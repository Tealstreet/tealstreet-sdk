# Address-keyed dispatch nonce domains

Owner: codex-uufub4, by direct overseer assignment. Required reviewers:
claude-m26ps4 (credentials), claude-19pt8s (AsterDEX V3) and codex-zs5e83
(Hyperliquid). This replaces the earlier assignment of Hyperliquid's nonce
subject to m26ps4; it does not transfer either venue's adoption. WEEX and
Coinbase reviews returned before this work began. Immutable planning base:
`da369f632b9c08c63aa94bf393583b1bf8d1e2ca`, actual Rust landing branch
`origin/feat/rust-rewrite`. Dedicated branch: `feat/rust-address-nonce-domains`.

Status: credential and both consumer PLAN directions reviewed; overseer
authorized local originals while the carries return. The initial checkpoint
changed only this document and its index row; the subsequent original target
adds no runtime, Cargo, provider, host, consumer or inventory change. Source
activation, push, CI and shipping size results remain separate gates.

## Source implementation checkpoint

The overseer authorized immediate source implementation and PR review instead
of further API-plan round trips. Freeze the owned-execution contract: separate
valid result and actual cleanup join, dormant factory, original promises observed
before start, 20s task bound, explicit joined reinitialization, unconditional
provider defaults and Arc-wrapped one-shot retirement. This supersedes the earlier
plan text requiring another direction round before source. Source, tests and
shipping evidence are reviewed on the PR; hosted activation still requires the
actual shared peer/resource proof.

The source adds the engine-owned setup table, private slot/ref/domain/issuer
bindings and readonly identities, address claim minting on the existing counter,
typed selection errors, LocalSigner owned PublicAddress execution, the typed
Managed/Session/Setup metadata union and business-byte-preserving serialization,
and negotiated host initialization/retirement entry points. It enables no venue.
The hosted-envelope owner implements the ONE peer extension: publicAddressOwned,
a distinct setup-joined acknowledgement correlated to original id/task/setup
generation/authenticated slot and binding incarnation, maximum2 retained owners
per account. Responses, errors, disconnects and reconnects never prove join.

The new host's unsupported retirement action disposes explicitly. Other unknown
actions keep their tested batch-tail behavior. New host export presence never
negotiates support. Existing business host metadata stays closed until its owner
composes the separately reviewed setup factory/transport; no old host receives
setup work without explicit trusted support negotiation.

## Existing authority and the precise extension

The #2093 dispatcher already reserves once during an eligible recipe prepare,
records the exact reservation, validates provider application and performs a
released-floor check with final Claim. It already has immutable attempts,
one-use claims, decimal-string receipts and bounded pre-send repair. Reuse it.
Do not add another counter, venue allocator or provider-supplied nonce.

`NonceDomain` currently has private name/floor/limit fields. `KRAKEN` uses a
trusted stable-id subject; `NonceClaim::mint` currently requires that subject
for every domain. `HYPERLIQUID_AGENT` already exists for the callback-time Cx
allocator, keyed by the public agent address. Aster V3's current recipe instead
calls its one interim per-signer allocator at preparation. Neither existing
path supplies the proposed address-bound dispatch claim.

The shared extension adds an address subject strategy and reviewed address
domain declarations. Stable-id Kraken behavior and its existing lifecycle
controls remain unchanged. Address subjects never fall back to stable_id,
credential text, a slot name, a KeyRef, a secret digest or a configured account.
An address proves no account permission, role, Session or transport authority.

## Domain policy and namespace continuity

| Domain | Subject | Clock floor | Limit | Sharing boundary |
| --- | --- | --- | --- | --- |
| Existing KRAKEN | Existing tagged stable-id key | RawMs x1000 | u64::MAX | Existing Spot/Futures contract, unchanged |
| Existing HYPERLIQUID_AGENT | Verified actual agent's 20 address bytes | RawMs x1 | 2^53-1 | Same agent across engines, vaults, networks and TxFlow in one process/WASM memory |
| Proposed ASTERDEX_V3 | Verified actual signing agent's 20 address bytes | ProtocolMs x1000 | u64::MAX | Same actual signer across users, engines and networks in one process/WASM memory |

The Aster consumer confirmed this clock/limit and native process-local sharing
against actual `interim_nonce`/`next_nonce`. Use the same dispatcher-calibrated
protocol observation: `stamp_ms()` floors DispatchContext.now_ms, and the nonce
floor is that integer stamp multiplied by1000 with checked u64 arithmetic.
Preserve its overflow refusal through the consumer's SignError::Unavailable
projection. ProtocolMs must never silently become RawMs for this consumer.
Keep Aster V1 HMAC claim-free. Do not require a public-address operation for an
HMAC identity or change its construction/refusal behavior.

With Aster's auth wallet, trusted setup aliases the read slot to the primary's
signing ref and signer address; both selected slots must share one sequence.
Without that wallet, independently configured read agents use their own exact
address/ref pairs. Each slot still passes the same engine-owned address binding
operation; configuration text alone cannot install a verified subject.

The overseer ruled that a configured Aster V3 signer address differing from
the selected signing key's verified address must refuse at trusted setup with
a clear typed error naming the mismatch. Never silently substitute the derived
address for mismatched configuration. Refuse only that slot's private address
path, typed NotSent before business signing, without an auth-health latch;
public work continues. The consumer reads the sealed verified address during
trusted startup before enabling that private path. Do not fail all startup or
defer this known failure into per-request InvalidSubject. Matching
configurations and legitimately
installed alias slots retain their behavior. The consumer must pin native
venue refusal versus the new earlier setup refusal with an original and record
the timing/diagnostic change in CALLERS. The generic mint still accepts no
configured address and gains no exception for mismatched input.

Canonicalize a validated address to exactly20 bytes. Spellings allowed at the
consumer's existing input boundary must identify the same bytes; there is no
new permissive native-config parser. The resulting NonceKey must be exactly
the current SHA256 of `0x` followed by40 lowercase hex digits. In particular,
Hyperliquid's domain name and key bytes remain unchanged. A new type tag on
that address hash would reset its sequence and is forbidden. Kraken's stable-id
tag remains separate, including a stable-id whose text resembles an address.

Use the existing process/WASM-memory NONCES store, never an engine-local map.
State survives engine/provider recreation. Different address domains remain
separate even for the same address; different agents remain independent.
Vault, wallet/user, API credential and chain configuration do not replace the
actual signer subject. No cross-process or separate-WASM-memory coordination
is claimed by a process-global mutex.

## Trusted address binding, before start

m26ps4's direction at the planning base confirms no existing carrier. Phase3
#2175 adds SetupFacts and a private signing-not-configured flag, not an address.
Consume its ACTUAL landing before source touches Identity/setup; do not build
on its unpublished head or duplicate its private flag.

Under a non-default address-nonce feature, Identity gains a private
`verified_address: Option<VerifiedAddress>`, marked `#[serde(skip)]`, with no
public setter. The binding participates in equality and contains exactly:

- The20 public address bytes.
- The exact selected signing KeyRef from which they were obtained.
- Algorithm::Secp256k1Digest.

There is no secret in this carrier. Debug may show the public address but must
not disclose its binding KeyRef. Serialization or an incoming wire object
cannot create the binding; deserialized identities need trusted initialization.

Every provider follows ONE engine-owned pre-start PublicAddress operation for
the exact installed signing ref. LocalSigner answers that operation
synchronously using its existing declared-key primitive; hosted/asynchronous
providers answer the same operation. Only the engine seals the validated20-byte
result into the identity. There is no separate LocalSigner install path,
cross-crate address bridge, public setter or raw-result injection. This is an
assertion by the already trusted provider, not independent proof of possession
of its secret. Neither credential presentation nor a configured address is a
setup answer.

The engine-owned pre-start operation and sealing are mandatory m26 source-review
gates. Setup must carry a real initialization generation/correlation; it cannot
fabricate a business Attempt or Session.
Changing provider, identities, signing refs or algorithm before start invalidates
both pending answers and installed bindings. Duplicate, late or foreign answers
cannot bind a replacement. After start, existing identity/signer sealing stays.

Each slot gets its own validated binding. A deliberately shared signing ref may
produce equal addresses, but an unverified secondary cannot copy a primary
binding to become eligible. Before inspecting a binding, claim selection asks
for the selected identity's signing_ref and preserves phase3's typed absence:
Optional absence is SigningNotConfigured (InvalidInput/NotSent, no auth latch);
MissingKey remains MissingKey where already applicable. Neither produces a
binding, reservation or provider signing. InvalidSubject is reserved for a
PRESENT signing ref with a missing, stale or mismatched binding. The exact
selection error carrier/mapping must preserve that distinction at source.
Public-only or otherwise unused identities must not gain an unwanted signing
requirement or setup call. Consumer eligibility precedes required attestation.

If a declared address-domain slot has a signing ref but its provider refuses or
does not support pre-start PublicAddress, record a typed SETUP-CAPABILITY refusal
for that slot's private address-signing path before start. The path reports
NotSent without latching authentication health. Public and claim-free work
continues; do not globally disable the engine or revoke an otherwise usable
identity. Do not defer this known setup failure into repeated per-request
InvalidSubject errors. Keep setup-capability refusal distinct from typed
signing absence, invalid subject and a later business signing refusal.

## Declaration and selected claim

Proposed narrow declaration, subject to exact source review:

```rust
// Feature-gated, object-safe default method; no associated const.
fn address_nonce_domain(&self) -> Option<&'static NonceDomain> { None }
```

Capture the venue declaration once during trusted initialization and seal it
at start. An instance method permits Aster V1 to return None while V3 declares
ASTERDEX_V3. Hyperliquid declares HYPERLIQUID_AGENT when its installed consumer
adopts. Reviewed engine constants fix names, clocks and limits; venue options
cannot create arbitrary domain names or change their arithmetic.

Keep existing `Slots<Identity>::select_with_nonce` and the recipe-owned
NonceClaim. Mint from the exact immutable member selected by Slots, according
to the domain's subject strategy. For an address domain, resolve typed signing
absence and any recorded setup-capability refusal first, then obtain only that
member's verified binding and check exact signing ref/algorithm. Stable_id is
neither required nor accepted as a substitute. Do not add an API accepting an
arbitrary Selection plus an unrelated Identity or address.

Before accepting/reserving an address claim, require the same declared domain,
installed provider binding, selected slot/credential/signing refs, role-derived
egress and current owner/attempt. A foreign domain, missing declaration, stale
binding, changed ref or swapped slot fails closed before reservation or private
work. A declared address path cannot opt into a stable-id claim. Existing
claim-free unsigned work remains claim-free; selecting a domain is not itself
permission to send. Keep current dispatch_nonce/NonceUnscheduled admission and
the existing refusal of nonce-bearing atomic barrier groups.

The precise representation and all validation entry paths are source gates.
The address feature must not enlarge Identity or dispatcher state when off.
No crypto dependency is introduced from engine back into crypto.

## Reservation, release and repair stay #2093

Reserve only in the genuine dispatcher preparation after all available subject
and route validation, never at venue queueing or setup attestation. One
preparation records one reservation; duplicate reservation or another claim
within it refuses. Preserve checked integer arithmetic and the existing raw
receipt-clock ceiling. Protocol calibration affects Aster's declared floor,
not Hyperliquid's RawMs floor. StampFloor is separate from a nonce counter.

Failed or abandoned reservations leave gaps; reserved maxima never roll back.
Reservation alone does not supersede an earlier attempt. Only exact attempt
replacement or the committed released floor can make its output stale. Fence
before provider apply and again at final Claim. Validate all fallible pacing,
journal, owner, route, deadline and binding work before the private atomic
released-floor/Lifecycle/pacing commit. No callback/allocation under the mutex.

WaitUntil adds no reservation itself and commits neither released-nonce nor
pacing floor. Stale repair may reserve a new nonce and recompute the actual
preimage/signature. Retain managed Attempt, owner, original deadline, selected
binding and cumulative maximum8 nonce repairs; no budget renewal or extra Plan
credit. Old preparation/permit/provider output is inert. A release-domain FIFO
head keeps its place during reprepare; this seam adds no ordering scheduler.

The new INTERNAL SignReceipt.nonce records the reserved u64 as a canonical
decimal string for provider/dispatcher validation. This is not a new field or
encoding on either venue's native protocol. Aster retains decimal digits in
its query and the same32-byte big-endian nonce word in its EIP-191/712 preimage;
its current native recipe has no nonce receipt. Hyperliquid retains its native
JSON safe-integer nonce and u64 big-endian action-hash bytes. Release, receipt
and the native preimage/wire must describe the same exact reserved value.
Hyperliquid's safe-number bound remains enforced; Aster values above2^53 stay
exact decimal/u64 values. No rounding through f64,
new receipt schema, blind digest, provider-selected nonce, physical retry or
automatic replay of a possibly-sent mutation is introduced.

## Consumer handoff is a separate activation gate

The shared seam does not convert native Hyperliquid/TxFlow or replace Aster's
interim allocator. Only consumer-owned source cutovers remove those callers.
Sharing Hyperliquid's existing key preserves reserved high-water; it does not
make old unfenced prepared/sent work obey the new released-floor fence.

Aster additionally moves a separate, unpersisted per-signer map. Its consumer
deletes that allocator in the same build that adopts the engine domain. This
code cutover must not be described as a floor-based continuity proof: at
protocol stamp100, legacy reservations100000 and100001 followed by a fresh
counter at the same stamp repeat100000. Fixed or regressing clocks and bursts
make the same issue explicit. Address normalization also intentionally merges
old differently cased signer-string namespaces; it cannot reset a live maximum.

There is no new cross-restart persistence promise. The Aster consumer confirmed
its activation contract: cold whole-process or WASM-instance replacement, with
no surviving old issuer/prepared local work and no old/new allocator coexistence
in one instance. That retains native's unpersisted-reset boundary without an
invented numeric seed, import or drain API. Reads re-sign fresh after restart;
writes are not resent across the boundary.
If live old/new state or owned work survives within a handoff, its continuity
and retirement/fence disposition remain source/adoption gates. Deleting a map
in a new build alone proves neither retirement nor clock dominance. Preserve
both maxima already present in the target. Any required drain/import authority
is separately reviewed, not fabricated by this subject carrier. A released or
possibly-sent mutation is never replayed, and no remote nonexecution proof is
inferred. Arbitrary external issuers remain the disclosed native exposure.

## Required originals and source gates

1. Actual setup/provider address binding: local scalar derivation equals the
   engine-sealed binding for the same input, and local/async providers use the
   same engine operation for exact selected refs; malformed lengths, wrong
   algorithm/ref,
   unsolicited/stale/duplicate setup answers and serialized forged metadata
   cannot mint. Provider/identity replacement invalidates old proofs. Missing
   signing preserves MissingKey; optional absence preserves
   SigningNotConfigured/InvalidInput/NotSent without an auth latch. A provider's
   pre-start Unsupported/refusal produces the slot's setup-capability NotSent
   outcome before private activation, while public/claim-free work remains
   usable. It is not a stream of later InvalidSubject failures.
2. Primary/secondary selection and alias controls: same actual signer across
   different credential refs/stable IDs shares a sequence; different signers
   with equal stable-id text do not. Swap slots/ref/provider/domain and require
   refusal before reservation, provider signing, I/O or publication.
3. Address canonicalization and continuity: supported hex/prefix spellings,
   malformed inputs and exact equality with the existing HL key. Alternate
   existing Cx and new claim reservations, recreate engines, and include TxFlow
   and multiple vaults/users without resetting either target maximum.
4. Family arithmetic: raw versus calibrated clocks, fixed/regressing clocks,
   more than1000 reservations per millisecond, invalid/nonfinite/negative times,
   fractional flooring, scaling overflow, MAX_SAFE and u64 exhaustion. Same
   agent across permitted network variants shares the consumer-pinned domain.
5. Genuine managed Prepare/Sign/Bind/Claim: reversed provider/claim order,
   abandoned larger reservation, stale apply and stale Claim, current retry
   versus forbidden Unknown replay, duplicate/expiry/cancel and cumulative8
   repairs. Retain exact provider/native digest and full request-wire oracles.
6. Pacing composition: WaitUntil changes neither released nor pacing floor,
   retains existing reservation, and rechecks freshness on later Claim.
   Domain-head reprepare keeps ordering; failure retires only exact resources
   and recovers credits. Positive-gap/nonce barrier refusals remain intact.
7. Stable-id Kraken and feature-off controls: existing originals unchanged,
   no new address requirement for Kraken, V1 HMAC or non-adopters, dyn Venue
   remains usable, and no setup-triggered business signing/nonce allocation.

Missing proposed symbols are not compiling-original RED evidence. Use genuine
existing primitive/dispatcher boundaries for originals; introduce new API-shape
tests with the reviewed source and disclose unreachable suffixes. No fake
Identity, slot, claim, setup answer, Context or predicted resource ID qualifies.

### Local current-API original checkpoint

`engine/tests/address_nonce_domains.rs` exercises actual Cx reservations from
engine start. Real joined child processes establish cold counter reset/repeat;
recreated engines in the same process establish retained live high-water.
The x1000 cold arithmetic probe uses existing KRAKEN with a public NonceKey,
not a fabricated ASTERDEX_V3 declaration, selected claim or Aster clock proof.
Address spellings, independent agents/domains, parallel engines, fixed/backward
clocks, bursts above1000/ms and the safe-integer limit use the existing store.

The proposed contract's hostile identities have NO verified binding: stable-id
text or serialized address metadata must not mint a claim, and missing signing
must retain MissingKey before subject validation. A genuine Kraken stable-id
selection is the working mint control. These cases compile on current
dispatch-nonce; the source adoption will move their enclosing feature guard to
address-nonce without weakening the assertions. They do not simulate successful
pre-start attestation or address-bound Prepare/Sign/Bind/Claim.

Existing dispatch-nonce-OFF controls retain unavailable dispatcher claims and
the ordinary Identity wire shape. The proposed address-nonce feature does not
exist at this checkpoint. These are behavior controls, NOT a byte-growth proof.
The fixed-budget paired WASM equality gate below is still mandatory at source.
Existing native/dispatcher originals, carrier/provider lifecycle controls and
all consumer financial/wire assertions remain separate required evidence.

Qualified local source is `8b0d46b6565ce0917fb3a8271ff11751e9dbe2c0`, plain
merged with actual `817a488c9d21444b223c8f99a33dc6d09ace31e3`. The scoped target
executes14 tests with dispatch-nonce enabled:11 passing controls and3 intended
assertion failures (unverified stable-id/serialized address mint and missing-key
classification). With dispatch-nonce disabled, all11 controls pass. Neither
mode ignores tests or counts a compile/setup failure as RED. Both strict scoped
Clippy modes, fmt and diff-check pass. A first strict run found only a redundant
helper return binding; the forward fix preserves every test body/assertion.
Each owned qualification target was removed after its mode completed.

The credential review correction consumes actual phase3 landing
`174f6a4c90c6a08dbabe72623fe3a4d8226109f2`. An additive Optional-signing
control uses CredentialSetupPolicy::apply_to_identities, confirms the genuine
SigningNotConfigured/InvalidInput/NotSent setup disposition, then requires nonce
selection to retain that error before the subject check. This pure selection
control does not itself prove managed authentication-health behavior; that
remains a source gate. The serialized-metadata control now requires successful
deserialization and equality with the ordinary identity before testing claim
refusal; a future unknown-field refusal cannot silently bypass the assertion.
The earlier 8b0 qualification remains attributed to its original source.

## AsterDEX V3 adoption (claude-19pt8s)

AsterDEX is the first production consumer. These pieces have landed:
- **Declarations:** `address_nonce_domain` (V3 only), `public_address_slots`
  (`Both`, or `Secondary` when public-data-only), `dispatch_nonce`, and
  `NonceUnscheduled`.
- **Sealing:** `start` rebuilds the V3 slots from `Cx::address_nonce_identities`
  and checks N1 per slot. Work issued before start seals on first use.
- **Recipe:** it mints a claim with `select_with_nonce`, reserves once per
  preparation, and echoes the nonce in the receipt. It takes the EIP-55 wire
  signer from `verified_signing_address`.
- **Registry:** the asterdex arm installs the identities and enables
  `address-nonce`.
- **Venue allocator:** deleted (`NONCES` x2 pruned).

It depends on the host start-site calls (owned by codex-uufub4). Enabling
`address-nonce` in the workspace grows every `Identity` by its inline address
capability. #2222 stores that capability boxed (`None` while uninitialized),
so unified venue enums stay under clippy's large-variant bound.

The parked-trade-versus-reads question is not reachable for AsterDEX:
- A parked trade holds no nonce, and reserves above the reads when it wakes.
- The local signer is immediate and unpaced.

The 8-repair budget therefore applies only to a future asynchronous V3
provider.

## Scope, feature costs and publication gates

The shared public-address carrier also has a claim-free GRVT consumer. GRVT
needs the actual selected Primary signing ref's sealed address before
CreateOrder apply/private egress, without a nonce domain or NonceClaim. Its
configured signer_address remains warning-only; Aster's mismatch refusal is
not a shared option validator. Credential-only login/cancel/leverage continue
without PublicAddress, and Optional absence retains SigningNotConfigured with
no latch. Hosted and local providers use the same genuine setup task, never a
builder business Read or a venue-owned binding installer. Setup-slot eligibility
therefore needs its own captured declaration, independent of optional nonce
domain selection. The exact API is under source review.

The overseer approved one finite raw 20,000ms setup bound per genuine task,
frozen at first emission; expiry is SetupCapability NotSent, no auth latch or
automatic retry. Explicit pre-start reinitialize advances the generation and
joins the actual old owner before launching new work. Timeout/abort wrappers,
detached callbacks, dropped futures and cleared maps prove no join. The provider
ownership contract must include actual work plus cleanup, including hosted
downstream work; abort-ignoring work keeps Retiring and blocks replacement.

fe7 accepted an unconditional public RetireAddressSetup Action variant and
opaque retirement payload, with construction/emission only behind the adopting
feature. Existing Sign/SignPayload bytes stay exact. Trusted pre-start setup
and retirement support negotiation precedes any setup task/signing ID; missing
support refuses typed before work. An old host receives neither setup nor
retirement. The new host's known unsupported retirement arm disposes; ordinary
unknown-op behavior keeps its existing batch-tail completion semantics. Exact
provider hooks, token/Action cloning, host integration and genuine join proofs
remain source-review gates, not approved runtime code.

The provider/host direction uses a dormant owned execution with separate result
and actual-join completion. Snapshot the original handle, methods and promises
once; attach observers to BOTH promises before start, including already-rejected
promises. Never re-read getters to substitute ownership. A binding needs both
the exact valid20-byte result and genuine cleanup join within the original20s.
Start, abort and report exceptions cannot skip independent cleanup or cause a
second report. Existing raced timeout/abort wrappers are not join evidence.

The public Signer trait's owned_public_address_capability and
create_public_address_setup methods have unconditional Unsupported/None
defaults and unconditional supporting public types. Only engine use and task
construction are feature-gated. Pin a downstream overriding provider compiled
with features unified both ways. The factory independently refuses malformed
input with None and zero emission: accept only one PublicAddress on its own
installed Secp256k1Digest ref, an empty receipt and no extra operation/payload.
Direct LocalSigner hostile-input originals must prove this without relying on
engine validation. Arc-wrapped retirement payloads are credential-acceptable
only with one-shot acceptance decided by the retained Retiring record; fe7's
exact public Action/Clone disposition remains a separate source gate.

Host originals must cover wrapper settlement before real callback/cleanup,
result-only and join-only completion, already-rejected promises, getter-based
owner substitution, thrown start/abort/report, and disposal while cleanup is
held. Missing negotiated support allocates no task/signing ID; ordinary unknown
actions retain their existing behavior. GRVT's claim-free setup is a new source
choice requiring actual originals, not evidence from its earlier builder Read.
The shared hosted-envelope owner separately proposes authenticated setup-owner
join evidence, exact correlation/capability and retained-state bounds. Ordinary
sign-res/error/disconnect, timeout, reconnect or a new engine cannot prove old
owner cleanup; missing acknowledgement keeps join pending and activation held.

Proposed non-default engine feature:
`address-nonce = ["dispatch-nonce", "sign-meta"]`. Gate runtime carrier fields,
subject strategy, declaration capture and setup handling. Public retirement
and selection-error variant sets stay unconditional, so Cargo unification
cannot change the cases seen by dependent crates. Feature-off shipping costs
must be measured, including unused public-variant and provider-vtable costs. This shared PR
enables no venue dependency feature and adopts none of the consumers.
Phase3 actual landing plus m26 and both consumer reviews precede source.

At first source, merge the actual Rust landing tip. The original zero-growth
gate was superseded by the overseer after the paired default-off Bybit build
measured1,444,849 ->1,445,286 (+437 bytes). The stable additive public API
residual is accepted; do not hide it with unrelated shrinking or continue byte
hunting. Retain this pair at its exact baseline/composition pins. CI supplies
the family records; one LOCAL current composed combined build is required.
The current combined cap is14,680,064 bytes after #2194. Test the address
feature on separately, including dispatch-nonce/serial/selection modes;
do not relabel a test harness as consumer adoption or current WASM execution.
Consumers later enable the feature in their own measured family/combined PRs.
Keep fixed budgets and current counterbalanced Bitget CPU policy, no overrides.
Delete each owned scratch target after measuring; retain sources/artifacts/logs.

Scoped originals and strict checks, existing nonce/credential controls, relevant
feature-mode rows, fmt, all bans/ratchets, immutable source/fixture/map review,
frozen-head CI and auditor GO remain required. CI records family sizes, never
the local combined artifact. The planning checkpoint has shipping delta0 by
construction, with no measured runtime or execution claim.

Source ownership pins: each domain declares a private StableId or Address
strategy; generic minting never infers it by excluding Kraken. A LocalSigner
factory derives public bytes while borrowing its declared store entry and
returns a dormant result-only handle, never another scalar copy. Native run
retains cleanup through observer unwind and cancellation; a continuing runtime
polls the original driver, while only its actual completion proves settlement.
The sign-meta business carrier is a separate fe7-owned prerequisite PR; this
address seam contributes only the genuine setup-table accessor branch and its
setup construction on that carrier.

The accepted +437-byte pair is scoped to the automatic a982+exact #2199
prerequisite baseline dcd6f797067 and composed60008f49266. A single later
Bybit method-order pair checks appending the new Signer hooks after the old
methods; it is diagnostic, not a new zero-growth hold. Source composes on
actual #2199 landing d3756f2a5fe, with its business carrier incoming-exact.

The completed LOCAL combined measurement is scoped to008c0daf732 (tree
961f6a864d5), on actual393b7668194:14,041,254 bytes, leaving638,810 bytes
under14,680,064. Optimized Node/Web outputs are identical, SHA256
4829ac9157a98afc5f780749eaac11b76f4152ca1dac7334cfe7cc7f9e997611.
All6,094 archived tracked blobs matched that tree; the owned target was deleted.
This is not a fresh measurement of the later Bitget/Phemex composition. Their
incoming changes and the exact source-preservation packet go to the auditor
for bounded qualification/measurement carry before READY.

### Automatic host initialization

The native Host, frame-server account actor and typed WASM engine initialize
address setup immediately before their existing engine start/seal boundary when
`address-nonce` is enabled. A synchronous LocalSigner completes its genuine
PublicAddress result and cleanup in that initialization turn, so declared slots
are verified before `Venue::start` reads installed identities. Each aliased slot
receives its own seal. A completed explicit initialization is reused.

This wiring does not declare external setup support. An unsupported provider
retains the typed per-slot refusal, and public/claim-free startup continues.
Asynchronous providers still require the explicit pre-start initialization and
actual result-plus-join handshake before the caller starts the engine. Starting
with unfinished setup retains the existing conservative retirement/refusal;
there is no blocking startup scheduler, automatic retry or hosted activation.
Initialization errors are reported through the existing StartupFailed notice.
All runtime additions are compiled only with `address-nonce`; CI records family
sizes and an exact combined comparison verifies non-adopter shipping growth.

The private capability uses an optional boxed state: uninitialized identities
hold no allocation, while verified/refused slots retain the existing exact
proof/error. This keeps Identity compact when Cargo unifies `address-nonce`
across non-adopting venues; BingX/Phemex read custody must remain within clippy's
200-byte variant-size difference. Slot/ref/domain/issuer/generation equality,
serde omission, resets, setup ownership and final-Claim checks are unchanged.

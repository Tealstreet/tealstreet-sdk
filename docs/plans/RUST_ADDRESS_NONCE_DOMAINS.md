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

## Scope, zero-growth and publication gates

Proposed non-default engine feature: `address-nonce = ["dispatch-nonce"]`.
Gate the carrier field, subject strategy, declaration capture, setup handling
and checks so ordinary builds retain their exact layout and code paths. This
shared PR enables no venue dependency feature and adopts neither consumer.
Phase3 actual landing plus m26 and both consumer reviews precede source.

At first source, merge the actual Rust landing tip. Measure paired default
Bybit, Bitget and stable-id Kraken families and one LOCAL composed combined
build; non-adopters must have zero byte growth with the feature off. Test the
address feature on separately, including dispatch-nonce/serial/selection modes;
do not relabel a test harness as consumer adoption or current WASM execution.
Consumers later enable the feature in their own measured family/combined PRs.
Keep fixed budgets and current counterbalanced Bitget CPU policy, no overrides.
Delete each owned scratch target after measuring; retain sources/artifacts/logs.

Scoped originals and strict checks, existing nonce/credential controls, relevant
feature-mode rows, fmt, all bans/ratchets, immutable source/fixture/map review,
frozen-head CI and auditor GO remain required. CI records family sizes, never
the local combined artifact. The planning checkpoint has shipping delta0 by
construction, with no measured runtime or execution claim.

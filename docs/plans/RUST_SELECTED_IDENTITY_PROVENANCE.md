# Trusted immutable selected-identity provenance

G2c plan-first checkpoint, 2026-10-24. Owner codex-hcmq3f; Plan API review
codex-uufub4; Selection helper coordination codex-9czdjh; first consumer review
claude-4yjlw0. Base actual a828ede7ebb54bc9c032ea61365213abac5e3dca. This is a
plan and existing GREEN controls, not a provenance implementation or original RED.

## Existing evidence and owner decision

Selection currently binds slot, route and account-evidence scope. Identity and
Credential contain opaque refs; Context proves its issuer, owner generation and
slot/ref. None proves the trusted account subject or a configuration snapshot.
A non-Clone wrapper can preserve those values but cannot create missing evidence.
The selected-read helper remains small and does not implement this prerequisite.

The overseer explicitly ruled: NO live configuration replacement inside one
engine is required. Account setup changes create a NEW engine; teardown retires
the old snapshot. Preserve the existing set_signer-after-start refusal. Do not
add a live replacement method or a mutable venue generation counter to create a
synthetic RED. G2c starts with immutable-per-engine trusted registration.

Existing setup-provider immutability and foreign-issuer refusal are GREEN
compatibility controls. Snapshot install/retirement are NEW-CONTRACT tests after
API review, not a missing-method compilation failure masquerading as RED. A real
executed wrong-identity path may be added as a RED only if independently found;
no current vulnerability is asserted merely because this new contract is absent.

## Trusted setup registration

Provisional vocabulary, exact API pending review:

```rust
// Final seal after host injections, before start; no venue can mint this snapshot.
engine.install_identity_snapshot(IdentitySetup { subjects, credential_policy })
    -> Result<SetupSnapshot, SetupError>
// Shared selection resolves an opaque binding from that installed snapshot.
// Admission couples it to this exact owner/leg; no arbitrary external Context.
```

SetupSnapshot is issuer-bound, engine-issued and opaque. It contains only refs
and non-secret declared/codec-resolved account data, never raw credentials,
private-key-derived hashes or a caller-chosen epoch. Identity.stable_id remains
the public nonce namespace; it is not a subject proof or configuration epoch.
Preserve existing externally/process-owned nonce stores and consumed evidence;
retain each family's existing per-engine counter initialization/reseed contract.
This slice adds no cross-engine persistence for per-engine venue counters.

Use the existing declared AccountSubject vocabulary from C2 in
RUST_CREDENTIAL_SELECTION.md: signer, trading_account, fee_subject, public_id,
kept separate. Primary and Secondary can have different signing keys; comparing
signer fields is not an account-equality check. Preserve the native codec's
canonical trading-account/portfolio identity relation. Address casing, exchange
UIDs, vault/application/account and fee subjects must not be interchanged or
selected from HTTP method. Never manufacture a subject from KeyRef/stable_id.

Exact subject codec inputs and the relation are an OPEN integration gate, not
implemented by this plan. Trusted setup/admission owners must supply the proper
non-secret account metadata once, with primary/secondary identity linkage. If a
required subject cannot be resolved before construction, that family's provenance
adoption remains blocked on an explicitly reviewed discovery contract; it does
not treat two missing values as equal or trust a venue counter. This first slice
adds no asynchronous setup/discovery seam and makes no new private-read readiness
claim for such a family. Host-declared metadata is trusted configuration, not a
claim of cryptographically proven account ownership.

Validate complete selected refs, slot/role and setup policy atomically before
installing or consuming IDs. Missing required refs/subject, conflicting slot
metadata or a second conflicting install refuses with no partially registered
identity. Existing #1899 set_identities capability registration must converge on
the SAME exact trusted identities; it is not a second subject/configuration
registry. Coordinate its actual constructor seam with claude-19pt8s/m26ps4.
Snapshot installation atomically converges with the FINAL #1899 trusted
identities, provider and setup policy as one validated construction tuple. No
snapshot survives a conflicting setup mutation, even before start. Installation
freezes ALL dependent setters: set_signer, set_identities, credential/selection,
capability and private-feed/setup policy setters, plus any derived setup caches.
Subsequent setter calls refuse without altering the installed tuple; exact setter
inventory is part of source review. Existing pre-start replacement remains legal
only while no snapshot is installed. Resolve trusted setup completely before
installation; do not capture one tuple and later finish a different one.
Failed installation leaves no snapshot, partial identity registration or frozen
half-state. Preserve provider capability checks independently at release.

## Binding, admission and release

The shared selected-identity binding retains a private reference to the exact
installed snapshot, slot, resolved account subject, Identity refs and routing
intent. Do not expose a constructor accepting raw epoch/issuer/subject proof.
One engine's binding is refused by another engine even if refs, subject text and
numeric owner IDs match. Selection happens once before admission; token movement
cannot reselect credentials, mutate the subject or widen the operation.

G2's recursive call declarations consume this binding and validate exact issuer,
snapshot, slot, permitted operation and configured account relation before IDs.
Every retry, fallback, Prepare/Bind, provider receipt, reprepare, group claim and
DATA path resolves that same binding. Existing Context owner generation remains
necessary and distinct from setup provenance. A new configured engine cannot
inherit a Plan, call credit, resend grant, snapshot handle or provider receipt
from the old engine. No deadline is minted or renewed by setup registration.

Secondary account reads retain the owner-decided C2 relation to the Primary
trading account. A mismatch is typed pre-send refusal, never silently served
from the wrong account. Which resolved fields establish that relation is reviewed
codec data; this plan does not implement an ad hoc all-fields equality test.
Mutation is Primary-only. Slot fallbacks preserve explicit absent-versus-empty
passphrase rules. Different fee/signer subjects do not silently alter read scope.

## Reconfigure means teardown and new engine

There is no live snapshot replacement API. Before exposing the newly configured
engine, the host tears down the old one: mark the old host/snapshot dead before
canceling its managed HTTP, provider jobs, sockets and timers. No new private I/O
from that engine is accepted after retirement starts. Late responses, receipts,
old timer events and retained handles cannot publish/authenticate or activate
new work under the new setup. Cancellation is not evidence that a released write
was unsent; retain caller receipt/Unknown/partial-ID semantics through existing
teardown and central owner retirement paths, not a local success terminal.

WASM EngineHost.dispose already marks disposed before cleanup, aborts requests,
disposes managed HTTP, cancels primitive/hosted signings and timers, and frees an
owned engine. RustEngineExchange.dispose also releases its setup/provider and
host. G2c integrates snapshot lifetime into those existing teardown paths; it
does not claim they currently implement snapshot retirement. EVERY host disposal must invalidate the snapshot before freeing an owned
engine or retaining a borrowed one, including borrowed WASM EngineHost (the
current default), not merely native borrowed-engine hosts. Disposed guards fence
host callbacks but alone do not retire a retained Rust engine's snapshot.
EngineHost.invoke refuses after disposed=true, so use an explicit private
teardown call path after public access is fenced; do not reopen normal invoke.
The exact shared failure-only cleanup hook is an API review gate. It must retire
owners under registry authority even after public validation refuses the snapshot.

Engine-owned bounds and existing resource limits cap retained stale handles;
there is no unbounded registry of all past configuration epochs. Dropping the
engine invalidates its issuer; asynchronous callbacks must be canceled/fenced by
the owning host. Account restart cannot erase external unresolved write evidence
or replenish a reconciliation/resend grant. P6b imported provider refs retain
its own session/attempt retirement requirements, and P4 public metadata must use
the same installed identity/role channel rather than a second subject path.

## Tests and sequence

1. Commit this plan and current GREEN controls; obtain core/consumer/setup fit.
2. After exact API review, NEW-CONTRACT controls: install once atomically with
   final identities/provider/setup policy; every dependent setter refuses both
   before and after start without mutating that installed tuple; failed installs
   neither partially register nor freeze setup;
   missing/conflicting subject or refs refuse before IDs; foreign issuer/snapshot,
   slot/role/ref mismatch; immutable binding survives queueing but not teardown;
   old Prepared/Sign/Bind/DATA/timer after new engine cannot release/publish;
   same refs and numeric IDs do not restore authority; native and WASM teardown
   converge, including disposing a borrowed host while retaining its engine; old
   admission/Prepare/Bind/DATA refuse while private cleanup can still retire; accepted/Unknown prior write evidence remains conservative.
3. Implement trusted registration and common admission/release validation, then
   compose G2 recursive bindings and G1 independent per-child Selection. No source
   hook is edited while frozen sibling PRs hold it; use actual landings and owner
   review first. No inventory prune in this engine slice.

Own module proposal request_context_identity_snapshot.rs; isolated tests
engine/tests/selected_identity_provenance.rs. Coordinate shared signing registry,
setup/admission mappings, engine lifetime, host disposal and managed validation
with their owners. No additional venue secret or reconfiguration state.

Focused engine controls, feature-off compatibility, scoped strict Clippy/fmt,
bans/ratchets; native first-consumer gates on adoption and exact-head CI for
WASM/size/replay/CPU. Source/caller/full-map independent reviews. No merge
without an overseer window. Architecture gate: one trusted identity provenance
and lifetime choke point, not a private wrapper claiming facts it cannot prove.

## Setup-freeze refinement

Core review requires installation, rather than start, to freeze dependent setup.
A third existing GREEN control documents that set_signer currently permits
replacement/removal before startup in an unregistered engine. It is evidence
for the NEW-CONTRACT boundary, not a fabricated snapshot RED. Reconfiguration
remains teardown plus a new engine; existing external nonce evidence and
per-engine reseed contracts remain unchanged.

## Concrete API proposal for core and teardown review

Handoff owner is now codex-mxb4mq. This proposal is based on local integration
`31de004fd11` (actual `59f50e4f37b`); no runtime API below exists yet. Reviewers
are claude-fe7lgm and codex-uufub4. G1's proposed constructor at `cb4bb676cca`
still accepts plain `SelectedRead` and proves no subject/configuration facts.
This proposal adds that witness; it does not retroactively strengthen G1.

### Construction and immutable witness

Names and signatures proposed for `engine::identity_snapshot`:

```rust
pub struct IdentitySetup {
    pub credential_policy: signing::CredentialSetupPolicy,
    pub subjects: SubjectSlots,
}
pub struct SubjectSlots {
    pub primary: Option<AccountSubject>,
    pub secondary: Option<AccountSubject>,
}
pub struct AccountSubject {
    // All fields are private; constructed by the reviewed subject codec.
    signer: Option<SubjectId>,
    trading_account: SubjectId,
    fee_subject: Option<SubjectId>,
    public_id: Option<SubjectId>,
}
pub struct SubjectId { /* private codec domain, environment, canonical bytes */ }
pub struct IdentitySnapshot { /* private Arc<InstalledSetup> */ }
pub enum SetupError {
    Started, AlreadyInstalled, Retired, MissingIdentity, InvalidIdentity,
    MissingSubject, SubjectConflict, SecondaryRequiresPrimary,
    UnsupportedCodec, Policy, Capability,
}
impl<V: Venue> Engine<V> {
    pub fn install_identity_snapshot(&mut self, setup: IdentitySetup)
        -> Result<IdentitySnapshot, SetupError>;
    pub fn identity_snapshot(&self) -> Option<IdentitySnapshot>;
}
```

`IdentitySnapshot` may be cloned as a read-only witness, with no public fields,
constructor, serde implementation or caller-supplied issuer/version. Its private
allocation owns the final tuple and an irreversible live/retired bit. Every
binding references that same allocation. A retained `Arc` never proves liveness;
validation checks retirement and pointer identity with this engine's installed
snapshot, in addition to the existing Context issuer. One slot/subject cannot
be swapped into a witness after construction. Installation mints no HTTP IDs,
owner credits, deadlines, nonce reservations or provider signing jobs.

Installation is the final seal, not part of registry allocation/build. Registry
construction resolves trusted refs, subject inputs and policy, but mints no
snapshot. EngineHost then performs its existing `setEntropy`,
`setSigningEntropy` and `useExternalSigner` configuration while still unsealed.
Only after all of these succeed does the constructor call the proposed WASM
`sealIdentitySnapshot()` administrative entry; no start, admission, provider
job or HTTP work can precede that seal for a provenance-enabled engine. Thus
host injection remains legal, and every dependent setter freezes immediately
when the actual install succeeds. There is no installed-but-mutable interval.

The installation input contains only subjects and the trusted credential policy;
the engine captures its actual final registered identities, provider, host
algorithms, feed policies and account-startup setting from its configuration.
Entropy providers stay in their existing engine storage and are frozen in place;
neither their secret state nor a new seed is copied into the witness. Validate a
temporary complete tuple, including policy checks against a local identity copy.
The result must equal the exact trusted refs passed to the venue constructor;
do not normalize or install a different identity at this late boundary. Publish
the one snapshot only after all validation succeeds. No provider/ref/policy
replacement occurs as a side effect of validation. On every failure, existing
pre-install settings remain unchanged and legal to replace: no partial snapshot,
partial subject registration, consumed owner/ID or frozen half-state remains.
A second install always refuses, including an identical tuple; there is no
replace/uninstall/revive API.

Installation errors are explicit. A complete identity without its required
resolved trading-account subject is `MissingSubject`. A subject for an absent
slot, refs different from the registry's trusted identity, or unequal resolved
Primary/Secondary trading-account domain/environment/value is `SubjectConflict`
(malformed refs are `InvalidIdentity`). Secondary without Primary is
`SecondaryRequiresPrimary`, not missing-equals-missing or a fallback. The same
failure atomicity applies to all these errors. A public-only empty-identity
engine needs no provenance witness; a private family without resolved native
subjects cannot enable this contract until its discovery/codec prerequisite is
reviewed. No implicit legacy fallback satisfies a provenance-required caller.

For snapshot-aware WASM construction, `identitySnapshotRequired()` is a closed
capability established by the trusted constructor. A required engine must expose
`sealIdentitySnapshot()` and the reviewed private retirement path; missing hooks
are a construction error, never an optional skip. The constructor seals after
the final host injection and before returning a usable host. Native Host builders
likewise finish `with_signer`/entropy injection before `Host::run` seals and
starts; a direct native caller must explicitly install before start. A required
engine started unsealed returns typed NotSent startup refusal before venue
callbacks or IDs. Earlier fake engines with no provenance requirement keep
their existing host compatibility; they cannot produce a bound read.

### Exact dependent-setter inventory

All configuration refusal guards use one common predicate: startup has begun,
a snapshot is installed, or identity teardown has begun. They refuse before
mutation and preserve existing post-start error contracts where already typed.

| Existing Engine setter | Installed-snapshot behavior | Constructor consequence |
| --- | --- | --- |
| `set_signer(Option<Provider>) -> Result<(), VenueError>` | Refuse replacement and removal, even before start | Final pre-injected provider is captured at seal |
| `set_identities(Identities) -> Result<(), VenueError>` | Refuse every later registration | Registry's existing final trusted refs converge on that tuple |
| `set_host_algorithms(Option<Vec<Algorithm>>) -> Result<(), VenueError>` | Refuse capability replacement/removal | Capture host declaration with the provider |
| `set_private_feed_policy(PrivateFeedPolicy) -> Result<(), VenueError>` | Refuse | Capture final private-feed policy |
| `set_public_feed_policy(PublicFeedPolicy) -> Result<(), &'static str>` | Refuse, retaining its current error type | Capture final egress/feed ownership |
| `set_account_startup(bool) -> ()` | Change to `Result<(), VenueError>` and refuse | Update `registry::finish` and the two engine unit-test callers; never silently ignore |
| `set_entropy(Fill) -> Result<(), VenueError>` | Refuse replacement | Finish identity RNG injection before install; do not reseed at install |
| `set_signing_entropy(Fill) -> Result<(), VenueError>` | Refuse replacement | Preserve independent signing RNG and per-family initial seeds |

`set_log_level` and `set_book_deltas` remain mutable: neither selects a credential,
account subject, provider, private read source or setup capability. Protocol-clock
calibration, Store financial rows and runtime market caches remain runtime data;
they cannot mutate the installed tuple. No public setup-policy setter exists in
this source: credential policy becomes an immutable installation input. Any new
subject/selection/capability setter must use the same predicate. Host `with_signer`
and entropy builders retain their pre-install meaning and propagate refusal.
Provider capability checks at release remain independent; snapshot installation
does not guarantee that an external provider will remain available.

Registry source map: the existing trusted setup paths registering identities at
`registry/src/lib.rs` near 1183, 1274 and 1436, and shared `finish` near 195, must
converge rather than registering a second tuple. The registry resolves trusted
subject inputs before exposing a staged engine through `visitor.visit(engine)`;
that staged value has no snapshot/binding or private provenance authority yet.
The host completes its actual provider/capability/entropy injection, then seals
that exact tuple before start. No venue callback participates in sealing.
Absent codec/discovery metadata blocks that family's provenance adoption; this
PR does not claim its private reads are newly available. The constructor owners
must review those hooks before source edits; no runtime source has changed here.

### Subject codec and canonical account relation

Proposed codec boundary:

```rust
pub enum SubjectCodec { CoinbasePortfolio, GrvtTradingAccount }
pub enum SubjectEnvironment { Production, Sandbox }
pub struct AccountSubjectInput {
    pub environment: SubjectEnvironment,
    pub signer: Option<String>,
    pub trading_account: Option<String>,
    pub fee_subject: Option<String>,
    pub public_id: Option<String>,
}
pub fn resolve_subject(codec: SubjectCodec, input: AccountSubjectInput)
    -> Result<AccountSubject, SetupError>;
```

These are candidate native codecs for review, not cleared family adoptions.
Environment is a closed enum, never free text. Unknown serialized variants or
a codec/environment pair not supported by native setup refuse; any additional
environment requires a reviewed variant. Domain/environment are not inferred
from a user URL, arbitrary casing or string aliases.
Do not add a generic `String -> SubjectId` constructor. Codec domain and network/
environment are part of identity: same text in another venue/environment never
matches. The codec validates lengths, format and native canonical representation;
it must retain native case sensitivity unless the existing protocol explicitly
normalizes. No generic lowercase, Unicode normalization, wallet-versus-vault
substitution, address-derived signer hash or HTTP-method inference is allowed.
Coinbase's portfolio and GRVT's trading-account ID remain distinct domains.
Native codec owners must approve exact accepted forms before implementation.
Other families return `UnsupportedCodec` until separately reviewed; no default.

An absent or empty required trading-account input refuses. A missing optional
signer/fee/public field remains absent and cannot substitute for trading account.
Secondary account reads compare both resolved trading-account domains,
environments and canonical values against Primary; signer and fee equality are
not account equality. Neither two missing subjects nor matching opaque key refs
satisfy this relation. Preserve source-specific native account identifiers; do
not infer them from `Identity.stable_id`, API-key labels or local counters. No
async discovery, new key derivation, new signing or network call occurs here.

### Selection, G1 retention and G2 validation seam

```rust
pub struct IdentityBinding { /* private snapshot, slot, role, refs, subject */ }
pub struct BoundSelectedRead { /* private Arc<SelectedRead>, IdentityBinding */ }
pub enum BindingError { MissingSnapshot, Retired, Foreign, Identity, Subject, Role }
impl<'a, P> Cx<'a, P> {
    pub fn bind_selected_read(&self, selected: SelectedRead)
        -> Result<BoundSelectedRead, BindingError>;
    pub fn select_installed_identity(&self, role: CredentialRole)
        -> Result<IdentityBinding, BindingError>;
    pub fn validate_identity_binding(&self, binding: &IdentityBinding, context: &Context)
        -> Result<(), request_context::Error>;
}
```

Read binding consumes the exact existing `SelectedRead`, checking its complete
Identity (credential, signing, explicit passphrase, extras and stable nonce ID),
slot, route intent and account scope against the installed tuple. It never
reselects. Mutation selection always binds Primary; asking Secondary to mutate
refuses before allocating any ID. Validation checks this snapshot is live and
installed by this engine, exact role/operation, slot/refs/routing, and genuine
current Context owner/leg. It cannot convert a foreign, expired or retired
Context into authority. Secondary account relation is checked before admission.
Existing API cannot accept a caller-provided subject or binding constructor.

G1 follow-on changes its registration argument from plain `SelectedRead` to
`BoundSelectedRead` after coordinated source review. The task retains the same
bound read for its whole lifetime and every child; a fresh child Plan still has
a fresh owner budget, but not a fresh subject/configuration witness. No optional
witness, anchor credential or `Arc`-only compatibility token satisfies G2c.
G2b remains hcmq3f's source: its declarations/Prepare/Bind/DATA/receipt paths
borrow this witness through reviewed hooks; this PR does not edit those hunks
before agreement. Released writes retain Unknown/partial-ID/budget evidence.

### Failure-only teardown and host ordering

```rust
pub struct IdentityRetirement { /* private, engine-minted, failure-only */ }
impl<V: Venue> Engine<V> {
    pub fn retire_identity_snapshot(&mut self) -> IdentityRetirement;
}
// Private engine/registry hook; never a Cx or public Context capability.
fn finish_identity_teardown(&mut self, retirement: &IdentityRetirement);
// WASM administrative export, called by the host's private teardown path:
// WasmEngine.retireIdentitySnapshot() -> void
```

Retirement is idempotent and starts by irreversibly rejecting new normal engine
entries, snapshot bindings, owner admission and outgoing I/O. The private
retirement witness permits only existing failure/cancellation cleanup, even
when normal Context checks now reject the snapshot. It grants no signing,
transport, retry, success publication or new owner IDs. The registry retires
current owners, prepared/provider claims, sessions/groups, sockets and timers
through existing authority; it does not invent a successful write terminal.
All retained snapshot/binding handles then fail validation permanently. A new
engine, even with the same refs/subjects and matching numeric handles, has a
new issuer and cannot revive any old witness or consumed grant.

WASM order: both `EngineHost.dispose()` and `failEngine()` fence public ingress
(`disposed=true`) before entering the common `cleanup()`. Retirement belongs at
the top of `cleanup()`, after its local once-only guard and before even command
settlement/deadline release, socketFailures, HTTP, provider jobs, socket/timer
cleanup, signer clearing or owned-engine free. Direct cleanup must also establish
the public fence; `disposed` alone is not the once-only cleanup guard because
both entry paths set it first. The private retirement call must not use `invoke`, which
correctly rejects disposed hosts, or dispatch actions/callbacks from teardown.
It ignores any return value and never drains actions or normal engine callbacks.
Cleanup attempts all remaining releases if retirement, command settlement or a
resource release throws. It runs once for BOTH
borrowed and owned hosts; `ownsEngine=false` only controls `free`, not retirement.
A missing hook on a test double does not permit a snapshot-aware production host
to silently skip retirement; exact construction capability detection is a source
review gate. Current older fake engines without snapshots retain cleanup tests.

Native `Host::run` already returns its engine after stopping. Fence inbox/command
admission before invoking the same engine retirement hook, then abort existing
managed/provider/socket/timer resources before returning that retained engine.
Drop/unwind paths need the same retirement guarantee; do not perform normal
venue callbacks from Drop. Name an explicit cancellation control: poll a native
`Host::run` future with live HTTP/provider/socket/timer ownership, then drop that
future without reaching its normal return. A retained snapshot witness must be
retired and resources aborted without a new venue callback or success terminal.
Pair that with normal owned-drop and retained-engine-return controls; cancellation
is not covered by either of them. The safe owned-engine return/Drop source shape
(for example an `Option<Engine>` taken only after stop) needs exact source review.
Proposed private cleanup does not rely on a disposed
JS wrapper to prove Rust liveness. Native owned and retained-engine controls,
plus WASM borrowed/owned controls, are required before runtime readiness.

### New-contract control ledger and review gates

Existing three native tests remain GREEN compatibility evidence, not REDs.
The next controls are explicitly NEW-CONTRACT. First executable REDs can test
actual `EngineHost.dispose` with a declared `retireIdentitySnapshot` spy: borrowed
and owned hosts must call it once after the public fence and before any command
settlement, socket-failure/signing cleanup or other resource release/free. A
`failEngine` RED must cover that second entry into the same cleanup. Return an
action from the spy and assert no action dispatch/drain. A working control proves public ingress already refuses during
cleanup; another proves owned versus borrowed `free` behavior is preserved.
These are boundary-call contract tests, not proof of the unimplemented Rust
snapshot or an assertion of an existing wrong-account exploit.

After exact signatures are approved, native install/freeze/codec tests require a
compiled API scaffold that fails closed; they must reach assertions rather than
count missing methods as an original regression. No test-only epoch/state shim
may be claimed as an installed runtime witness. Cover failed atomic install and
subsequent legal setters, each of eight setter refusals before/after start,
foreign issuer and refs, absent/conflicting domains, genuine Secondary relation,
Primary-only mutation, same binding through G1 activations and teardown of old
Prepare/Bind/DATA/timers while released-write evidence remains conservative.
Exact host/registry/G2b hooks remain reviewer source gates before implementation.

## First new-contract boundary evidence

Core review of `09fec34606b` clears the first host-boundary controls only. Both
borrowed and owned `EngineHost.dispose` paths now have executable NEW-CONTRACT
assertions in `hosts/wasm/js/host-identity-provenance.test.mjs`. Each reaches its
final assertion and fails solely because `retireIdentitySnapshot` is never
called: the observed cleanup/free sequence otherwise matches. Four working
controls pass: public ingress is fenced during cleanup, borrowed engines remain
unfreed, owned engines are freed once, and a throwing resource does not strand
later cleanup. This is two assertion REDs and four GREENs on unchanged runtime,
not missing-method/import failures or evidence of an existing native snapshot.
No runtime source, subject codec, shared claim/group hook or host teardown path
has been edited. Exact Rust/registry/host source and native codec reviews remain
required before implementation. The hook-throw and poisoned-engine cases from
core review must be added at that source stage; these first controls do not
prove either property.

G1 constructor review adds one precise borrow requirement:
`BoundSelectedRead::selected(&self) -> &SelectedRead` is crate-private. It feeds
the existing child queue helper without unwrapping or dropping the binding.
Tasks and all children retain the whole bound value; there is no public method
returning an unbound owned read or optional/plain compatibility constructor.


## Review corrections: final seal and common cleanup

fe7lgm reviewed the first host controls CLEAR but withheld API clearance. This
revision addresses the construction conflict explicitly: registry build is not
installation. Existing host entropy and external-signer injection happen first;
one final install/seal then captures that actual tuple before start, immediately
freezing all eight setters. No installed snapshot is mutable. A constructor
control must preserve both distinct entropy inputs and external capability data,
then require sealing before start; no missing API compile failure counts as RED.

The shared cleanup top is the retirement boundary for both normal dispose and
`failEngine`, before even pending-command settlement. Expanded ordered controls
include pending write rejection/deadline cleanup, socket-failure timers and all
signing maps. The retirement spy returns an action and the test requires no
`run`/transport dispatch. Existing assertions remain. The borrowed/owned
`failEngine` variants are host-boundary controls only, not proof that calling
Rust after a WASM trap is safe. Exact safe retirement after poison, a throwing
retirement hook and all later cleanup attempts remain mandatory source-stage
controls; the implementation cannot simply re-enter a borrowed poisoned engine.
Native cancelled-run-future, owned-drop and retained-engine-return cases are
three separately required controls. Subject errors and Secondary-requires-
Primary are explicit and atomic; environment is closed, never a free string.

Revised API and construction/teardown hooks require both reviewers again before
runtime edits. Candidate native codec accepted forms still need their owners.

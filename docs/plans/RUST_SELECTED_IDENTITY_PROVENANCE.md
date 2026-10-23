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
// Trusted construction, before start; no venue callback can mint this snapshot.
engine.install_identity_snapshot(identities, subjects, setup_policy)
    -> Result<SetupSnapshot, SetupError>
// Shared selection resolves an opaque binding from that installed snapshot.
// Admission couples it to this exact owner/leg; no arbitrary external Context.
```

SetupSnapshot is issuer-bound, engine-issued and opaque. It contains only refs
and non-secret declared/codec-resolved account data, never raw credentials,
private-key-derived hashes or a caller-chosen epoch. Identity.stable_id remains
the public nonce namespace; it is not a subject proof or configuration epoch.
Nonce stores and consumed nonces never reset because an engine is recreated.

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
Preserve provider capability checks independently at release.

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
does not claim they currently implement snapshot retirement. Native borrowed-
engine hosts must explicitly invalidate the installed snapshot at teardown if
they retain the Rust engine for cleanup, without installing replacement config.
The exact native failure-only cleanup hook is an API review gate. It must retire
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
2. After exact API review, NEW-CONTRACT controls: install once atomically;
   missing/conflicting subject or refs refuse before IDs; foreign issuer/snapshot,
   slot/role/ref mismatch; immutable binding survives queueing but not teardown;
   old Prepared/Sign/Bind/DATA/timer after new engine cannot release/publish;
   same refs and numeric IDs do not restore authority; native and WASM teardown
   converge; accepted/Unknown prior write evidence remains conservative.
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

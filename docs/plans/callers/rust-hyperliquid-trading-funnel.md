# Hyperliquid local trading custody

Applies to Hyperliquid and HyperliquidSpot SDK, browser/WASM, worker and native
callers when the complete local trading cutover lands. A recipe checkpoint by
itself does not activate the venue. Hosted remoteSigner and payloadSigner remain
unsupported; this change adds no hosted path or TypeScript venue fallback.

Trusted setup consumes the agent scalar before constructing the venue. Generic
options retain only public account configuration and installed references. The
key, applicationId and vault options are public routing/account values, never a
presentable credential or a nonce subject. Callers must provision the local
provider in the executing host and complete engine-owned public-address setup
before start. Unsupported or failed setup refuses private work. Copying retained
exchange options cannot provision another signer.

Scalar text remains byte-exact: surrounding ASCII space, NEL and BOM refuse.
Hex decoding accepts an optional 0x/0X prefix and hex letter case; the provider
uses the same decoder on native and SDK/WASM paths. The old SDK Wallet's 0X
rejection is superseded by that shared native decoder. Invalid input produces
fixed errors without the scalar. Spot validates its configured agent against the
engine-proved address; perps keep the configured-agent mismatch warning.

Every write waits for real Prepare, provider completion, Bind and final Claim.
The command root is serial and unbounded; each physical call has a
20-second bound that includes preparation and cannot be renewed by pacing.
A 334ms mutation-only FIFO gap starts at actual release rather than eager queueing. Reads do not join
that FIFO. Builder readiness keeps its existing eight-second limit. Callers must
not treat a preparation or a provider result as evidence that a write was sent.

Unknown is never retried as a write. The existing fresh-order reconciliation
fence stays active until a complete authoritative read releases the relevant
scope. Structured accepted IDs survive mixed batches and later failures. Native
order values, bracket grouping, cancellation proofs and amend dependencies stay
pinned. Shared write-journal terminals replace numeric-ID completion authority.

The actual proved agent keys the existing Hyperliquid/TxFlow nonce namespace,
across engines, networks and vaults in one process/WASM memory. A new engine must
not inherit live legacy host work: cancel or fence its unsent release, join its
actual work/cleanup, then initialize the new engine. Disposal bookkeeping or a
shared reserved maximum alone does not prove retirement. Separate processes or
WASM memories retain their existing deployment coordination obligation.

This clears the local venue raw-scalar holder and eager HTTP/signing callers
only with the full cutover and its production-binding controls. Standalone
native signing/vector helpers still consumed by the disabled TxFlow family are
not an HL runtime fallback. Their separate deletion requires that consumer to
migrate or disappear. order_read_epoch adoption follows in a separate slice.

LocalSigner capability admission now reports Secp256k1Digest as supported when
its existing secp256k1 arm is linked and the exact key passes declared-algorithm
validation. This also affects AsterDEX and GRVT builds linking that arm; their
native suites and crypto feature-on/off controls qualify this shared correction.
It adds no key, provider operation or signing algorithm to a request.

The managed journal replaces three legacy projections explicitly. An HTTP
401/403/429 without proven native refusal evidence is Unknown, without replay.
Only a waitingForFill attachment of an accepted parent folds into that parent
using its real oid; independent native child oids remain real result IDs. A
standalone zero-size, reduce-only positionTpsl is declared Setting before issue:
waitingForTrigger succeeds with an empty ID result, without inventing a handle.
Ordinary/fixed-size or unmatched ID-less placements remain Unknown and reconcile.
No Spot contract is widened. Unlisted per-row errors are Unknown; a complete batch
with accepted rows plus the documented PerpMargin refusal has structured IDs
and no execution-uncertainty label (KnownPartial, outcome None). The literal
`Insufficient margin to place order.` is the cited proof; generic text is not.
See https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/error-responses.
The old SDK/native assertions for these projections are deliberately amended;
order prices, sizes, grouping, accepted IDs and no-retry assertions remain.
Preparation may start before the 334ms gap; the final release observes the gap.

This cutover qualifies a fresh host with no legacy owner. Warm replacement of an
old eager-signing engine is unsupported until its original provider/transport
promises and cleanup have actually joined and retained callbacks/timers are
fenced. The current synchronous dispose API does not expose that join, so abort,
map clearing, delay, or browser teardown cannot authorize replacement. Joining
already-sent work still leaves an uncertain result Unknown and reconciled.

Settings writes are marked uncertain at issue. A definitive delivered success
clears the mark; engine-owned unproven seals retain it. The existing reconcile
hook starts or joins the native settings read. Read revisions prevent a
pre-write snapshot from clearing a newer mark, including an in-flight read
joined by a reconcile re-ask. The order-census fence remains independent.

The former Sensitive options wrapper erased only public account data after the
scalar holder was removed, so that wrapper and its erasure implementation are
removed. Redacted Debug and the installed-options no-scalar controls remain.
Two startup-unit-fixture inventory hits describe trusted LocalSigner setup JSON
and policy metadata in cfg(test) startup_tests.rs. The production signer never
reaches these test-fixture setup inputs; they are not venue secret holders or
eager signing calls. The final lexical request fragments are removed by moving
builder/effect classification to pure enum predicates, with no Cx or issuing authority; the
native callback uses those predicates. The queued-read struct is destructured
at dispatch: its field named request was counted as a legacy member despite
being an enum continuation. Neither field nor receiver is renamed. The error
condition is expressed as the same empty-or-idempotent-success case.

The local-provider capsule omits only the trusted trimSigning false default for
both slots, so a standalone provider build without trimming can decode it. True
is still serialized; caller trimSigning metadata stays refused at either value.
The effective policy/readiness and scalar bytes are unchanged. The pre-existing
native from_setup feature-off handling of a true policy is a credential-core
follow-up; the only current true declaration (Bitunix) links that feature.

For HL, LocalSignerSetup.primaryComplete is false after the account key is
removed from the private capsule. It is not signing readiness: canSign uses
credentialPresence.secret; hosted guards and codecs do not admit HL.

Legacy top-level options.passphrase remains accepted and ignored for both HL
names. Trusted setup discards it before provider construction; it appears in
neither retained options nor the provider capsule. Explicit provider-setup
passphrases and secondary setup remain refused. No NX caller migration is needed.

Hosted payload refusal is evaluated against intact options and the actual alias
before local scalar decoding. Before the provider is installed, public intent is
publicMode.requested when present, otherwise publicDataOnly, matching Lighter
setup rather than inferring eligibility from missing provider references. Native setup still consumes and drops raw input
before surfacing that fixed Unsupported/NotSent error; public-only intent keeps
its existing setup path. This adds no hosted capability.

Deferred acceptance is a deliberate correction of this PR's earlier conservative
Unknown projection, matching the unchanged TS replay oracle. The command/wire
shape freezes eligibility before issue; response rows cannot relabel a Place as
a Setting. Physical row count and parent kind are validated before logical
projection. A cited child error keeps KnownPartial and its real parent ID; an
unlisted child error is PartialUnknown and fences continuation. Accepted
full-position settings remain effective across a later refusal, even with no IDs.
This also covers an amend's confirmed full-position replacement: its preissued
Setting succeeds with the replacement cloid and no fabricated native order ID.

A protective child that terminates before first appearing in the active store
emits the existing child_leg.failed event, correlated by symbol, native oid,
clientOrderId and native status; already-stored terminal removal is unchanged.
The native private row must identify a reduce-only trigger with a supported
Stop/Take-Profit type, a mapped symbol, a nonzero oid and a nonempty cloid.
No expiring placement record is required. Duplicate terminals are suppressed by
the existing oid tombstone before it is updated. This is bounded deduplication:
a complete census prunes tombstones older than 120 seconds, and a confirmed open
row in a later census can clear one after 30 seconds. A replay after that cleanup
can repeat the event; callers must not treat it as a second placement or resend.
The existing browser EngineHost event action reaches handlers.onEvent, and the
SDK RustEngineExchange.receiveEvent emits engine-event and writes warning/error
logs with the event fields. No new NX wiring is required. Parent acceptance is
not rewritten by the child's later lifecycle terminal, and no write is replayed.

Two native-construction refusal texts change deliberately under the cutover
(344b557f068), with InvalidInput and no admitted engine in both cases. A legacy
HLSpot remoteSigning:true input with key:"" and invalid secret:"s" now fails
trusted scalar setup as "Invalid Hyperliquid local signer setup", before the
old generic "Rust engine private accounts require a local API key and secret;
remote signing is unsupported" guard. Providerless injected HL/HLSpot references
now fail as "Hyperliquid requires its primary signing provider", rather than
"Injected signing is not supported for this venue yet". Native callers matching
those exact legacy texts must use the typed InvalidInput category or the new
fixed text; SDK beta hosted refusal remains Unsupported/NotSent. R1 corrections
d179009c0ce and 7ccda342f32 separately put payloadSigner refusal ahead of scalar
decoding and use explicit public intent before provider installation; they do
not enable or infer an injected provider from opaque refs.

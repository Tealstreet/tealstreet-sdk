# Engine socket lifecycle and typed authentication core

Owner: codex-5c57jl. Plan-first, read-only audit checkpoint from actual
`4a17998a4d9825dc7de303a23be793b5a7d9cabe`; no production changes yet.
This implements architecture audit section E (ranks 8–9), extending the existing
`engine::socket::Supervisor` rather than adding another venue helper. Initial
references are **OKX** and **Bitunix**, followed by bounded family adoption.
Existing signer checkpoints and hosted-presentation #1763 stay on their branches.

## Reverified boundary

| Current source | Existing responsibility | Responsibility still outside the engine |
| --- | --- | --- |
| `engine/src/socket.rs` | Connect/auth/idle deadlines, stable retry reset, jittered backoff, canceled timer recognition | No socket/feed/epoch identity; callers must report only current traffic and call authenticated/retry/cancel correctly. |
| `engine/src/engine.rs` (`Cx::connect/close`, WS event arms, `SocketRoles`) | Physical IDs, role health, close actions, signed-socket cancellation | WS events are forwarded to venue callbacks; role tracking is not an epoch/auth/replay authority. Retirement, readiness and invalidation remain adapters' obligations. |
| `venues/okx/src/lib.rs` + `account.rs` | Three connection slots, string-code login parsing, account bootstrap, topic batching, public book reset | Current-ID filtering, open/auth booleans, Supervisor integration, terminal startup refusal, replay and retirement ordering. |
| `venues/bitunix/src/lib.rs` + `account.rs` | Private/futures/public-shard codecs, boolean login ACK, native same-socket retry, topic batching | Active/open IDs, login/queue timer ownership, private first-real-pong policy, futures-book invalidation and current-interest replay. |

The current source census still finds Supervisor references in 26 venue crates.
This is evidence of duplicated policy/integration, not a claim of a newly proved
production defect. Existing auth/lifecycle tests and fixes remain acceptance
controls. In particular, Bitunix futures owns depth; retiring ticker shards must
not clear futures books. Existing OKX and Bitunix rejection policies differ and
must be declared explicitly, not silently replaced by a universal close policy.
Bitunix private currently has no auth deadline, retries malformed/false login on
the same socket, and does not wait for ACK before handling native server-push
data. Its private session has no subscription replay batch. Preserve these
compatibility distinctions as declared behavior; do not invent an auth gate,
first-pong timeout or private subscriptions while moving ownership. The first
real Bitunix private pong activates the 45-second idle guard; subsequent current
text traffic refreshes it, including malformed frames. Activation, refresh and
auth proof are separate. OKX liveness is pong-only and its auth bound is ten
seconds; neither setting is a universal default.

## Shared contract to freeze before implementation

- One engine-owned logical feed entry holds the current physical connection,
  an opaque checked epoch token, phase and Supervisor. A venue declares role,
  auth requirement, separate liveness activation/refresh proofs, retry delay
  policy and feed-owned state. Bound feed/interests/queued-frame/timer counts
  and use checked finite deadline arithmetic before freezing the public types. Overflow refuses replacement; socket zero remains
  the suppressed-public placeholder, not a live epoch.
- WS open/message/binary/close dispatch, scoped timers, typed auth evidence and
  outbound work validate that token. Retired/unknown epochs cannot change auth,
  health, timers, subscriptions or store state. Scope all adapter-owned login,
  heartbeat and paced-subscription timers through the same entry; no new local
  epoch counter or remembered-token checklist in each venue. Admission precedes
  health counters and private-traffic timestamps, not only venue callbacks. Keep
  an explicit legacy unmanaged compatibility path until each family is adopted;
  a managed-token check must not drop every non-adopted venue event.
- Codec evidence is `Accepted`, `Rejected` or `Malformed`, bound to the current
  token. Positive ACK schemas stay codec data: OKX requires explicit code `0`;
  Bitunix requires an actual boolean `result`. A missing field, malformed value,
  event name, heartbeat or unrelated success is never authentication proof.
  Separate duplicate acceptance from a new authenticated epoch; it must not
  replay subscriptions or restart account reconciliation twice.
- Rejection carries native typed error evidence; declarations distinguish a
  terminal startup key refusal, reconnectable loss/transient failure, and the
  existing coalesced same-socket retry (one pending two-second timer, no
  invented attempt cap or auth deadline). Malformed ACKs use the declared diagnostic
  and recovery policy, never key-rejection evidence. Preserve credential scope
  and startup-ready behavior; reconnecting alone must not heal auth. Check
  configured absolute deadlines before admitting ingress at or after expiry,
  even if timer delivery is late; an ACK/open/pong cannot rescue an expired
  phase. Keep the absent Bitunix auth deadline absent. Same-socket Bitunix ACKs
  carry no attempt correlation: admit by current physical epoch and idempotent
  state, not an invented local attempt number. Preserve OKX startup refusal
  retiring both private/business feeds as a declared auth-group effect.
- Retirement invalidates the epoch before closing/canceling anything, cancels
  signed upgrade/auth work and scoped timers/queued frames, runs feed-owned
  invalidation once, and only then permits retry/replacement. Reentrant/late
  signer completion and repeated host closes cannot revive it. Fence ordinary
  Cx.send/send_signed, queued WsSend/WsConnect actions and signing receipt/release
  callbacks as well as parked signing entries; close_socket alone is insufficient. Preserve the
  existing signed WS ordering and cancellation barriers; signing stays in the
  shared SignStep/provider path, never in this lifecycle core.
- Engine owns current subscription interests and replay entitlement per epoch.
  Venue codecs encode bounded native batches from those interests. Readiness
  grants one replay; duplicate open/ACK cannot grant another. Unsubscribe/change
  during backoff alters the next replay; superseded batch timers cannot send.
  Same-epoch paced batches also carry a current-interest revision so an already
  queued unsubscribe/change suppresses obsolete frames and admits later additions.
  Avoid storing signed payloads or replaying account mutations as subscriptions.
- Feed-owned invalidation is a required declared effect, run synchronously before
  replacement traffic: clear owned raw book/cache state and publish book_changed
  even if already empty. Feeds owning no invalidated state declare that explicitly;
  private account snapshots and ticker-only shards do not acquire depth ownership. Engine enforces invoking the owner effect once; venue
  supplies native book/cache access, not a second retirement algorithm. Keep
  private financial snapshots/freshness tokens separate from public depth policy.

Concrete public types and callback signatures remain provisional until the
core plan review and RequestContext/signing boundary agreement. The first core
must enforce these obligations at engine dispatch boundaries, not merely expose
helpers whose call ordering every migrated venue must remember. Inventory and
ratchet legacy socket owners during fan-out; no universal closure claim while
legacy adapters remain.

## Committable implementation sequence

1. Add engine lifecycle/evidence state and tests around existing Supervisor;
   narrow WS-only `engine.rs`/`lib.rs` integration. Make retirement, admission,
   timer ownership, replay entitlement and invalidation observable to test probes.
   Review exact signatures before touching dispatch or signing internals.
2. Adopt OKX public/private/business feeds and Bitunix private/futures/shards.
   Delete their migrated policy state, keeping codecs/native refusal mappings,
   URLs, credentials, signing recipes, feed ownership, batching and timeout units.
   Preserve OKX signer-checkpoint edits by real integration before final proof;
   coordinate Bitunix account sections with metadata/history/fill owners first.
3. Run original-code RED controls for each newly demonstrated defect, then the
   shared path and both family compatibility suites. At minimum cover missing /
   wrong-type / explicit rejected / accepted ACKs; duplicate acceptance; old ACK,
   pong, open, binary, close and timer after replacement; late parked signer;
   terminal refusal versus transient loss; legacy unmanaged compatibility;
   same-epoch queued-interest changes; first-pong compatibility; no auth
   deadline extension by pongs; exactly-once current-interest replay; canceling
   queued batches; clear-before-reconnect; one-sided depth recovery; sibling feed
   isolation; suppressed public feed; dispose and epoch/ID exhaustion.
4. Record same-PR caller section and audit ledger updates without claiming NX
   rendering adoption. Independent correctness and adversarial reviews answer
   the architecture gate, then focused engine/OKX/Bitunix/Binance native, strict
   Clippy/fmt/bans, affected host/SDK tests/typecheck/lint. CI supplies full WASM,
   all-family replay/size/timing evidence; no local release build or full replay.
5. Real-integrate required shared-engine/build/workflow changes before final CI
   and authenticated post-#1762 replay proof. Existing documented TS-only classes
   require fresh complete pre-change controls; all Rust outputs remain strict.
   Separate fresh own-only guard and merge under the owner's current queue.
   Fan out on fresh branches after the two references prove the shared contract.

## Ownership and limits

RequestContext owner confirmed WS-only SocketRoles/connect/close/event-filter
regions are clear. Preserve its managed HTTP Pending/route_ready/final-claim and
command-delivery code. Any signing_dispatch Pending/finish/release change needs
explicit coordination; no HTTP lifecycle edits belong here. Protocol timestamps
use the shared clock and signer; deadlines/backoff use raw host scheduling time.
No key custody, transient session-token store, remote wire or crypto extension is
invented here. If a reference requires one, report it as a shared prerequisite.

Architecture gate: this is an engine choke point replacing repeated policy,
with two different codec families as references. Per-family codecs are not an
alternate lifecycle, auth evidence aggregator, retry loop or epoch authority.
Plan validation is source inspection only; no new regression, live private
capture, implementation or cutover-readiness result is claimed at this checkpoint.

## Read-only plan review

Correctness and adversarial reviews confirm the engine choke-point direction.
Their source-audit corrections are incorporated above: distinct idle activation /
refresh proof, uncapped but coalesced Bitunix login retry, managed-versus-legacy
admission before health mutations, queued outbound and signer-release fencing,
same-epoch interest revisions, uncorrelated native ACK limits, OKX auth-group
retirement, bounded collections and checked deadline arithmetic. The next gate
is concrete API review and native original-code controls. The owner subsequently
authorized implementation through the two adoptions, review and a draft PR.

## Concrete API proposal (review before production edits)

New `engine/src/socket_lifecycle.rs` (exported through `lib.rs`) owns the following
non-secret types. Names are proposed; semantics below are the review contract.

- `FeedId`: opaque engine-allocated bounded logical-feed handle.
  `Epoch`: opaque `{ feed, generation, physical SocketId }`, with accessors only.
  An epoch is issued only when the registered feed is permitted to connect.
  Physical role is derived from immutable policy, never a conflicting SocketSpec.
- `FeedPolicy`: existing `socket::Config`, required/absent/observe-only auth,
  auth refusal recovery and group, idle activation/refresh proof, subscription
  cadence and invalidation declaration. No new backoff implementation.
  `AuthRecovery::{Reconnect, RetrySameSocket{delay_ms}}` plus declared permanent
  credential-refusal handling before native startup readiness. Native readiness
  is a read-only codec fact; lifecycle/group policy is engine-owned.
- `AuthEvidence::{Accepted, Rejected(VenueError), Malformed}` and
  `Control::{Other, Pong, Auth(AuthEvidence)}`. `Venue::socket_control(feed,text)`
  is a pure codec; it cannot update state or perform I/O. A read-only native auth
  context supplies startup-ready status where OKX's existing terminal decision
  depends on account bootstrap, without moving retry policy back into the codec.
- `LifecycleEvent::{Opened, Ready{reopened}, Ping, RetryLogin, Retired,
  AuthFailure, Reconnect}` with feed/epoch and typed reason as appropriate.
  `Venue::on_socket_event(cx,event) -> Result<(), SocketError>` handles native
  payloads, store clearing and
  native account resync; the engine has already made/committed the transition.
  A registered feed cannot silently omit its retirement effect: managed callbacks
  default fail-closed, while unmanaged families retain existing callbacks.
  Retirement acknowledgment is required: callback failure leaves the feed stopped
  and quarantined with no retry grant or Ready output. Group retirement marks ALL
  sibling epochs inactive before delivering any callback. A no-state feed declares
  that explicitly; effect success cannot be inferred from a void callback.
- `Interest { key, wire_args }` contains bounded non-secret native codec data.
  `Venue::encode_socket_interests(store,feed,subscribe,interests,emit)` produces
  native `EncodedBatch { text, covered_keys }` values through a bounded emitter; coverage must be exact,
  unique and contained in the requested operation. No signer invocation or wall
  clock is allowed. Engine tracks desired interests separately from queued and
  dispatch-released interests. A superseding revision recomputes from actual
  released coverage, retaining unchanged-but-unsent interests. Replacement is
  transactional: validate/encode/budget the complete update before committing or
  releasing any unsubscribe/subscribe; failure retains the old state. Engine owns
  diff, ready-once replay, revision, queue and cadence.

Cx facade:

```text
register_socket_feed(policy) -> Result<FeedId, SocketError>
connect_socket(feed, spec) -> Result<Epoch, SocketError>
connect_socket_signed(feed, spec, SignStep) -> Result<Epoch, SocketError>
socket_epoch(feed) -> Option<Epoch>
socket_authenticated(feed) -> bool
set_socket_interests(feed, Vec<Interest>) -> Result<ProposalId, SocketError>
retire_socket(feed, reason) / stop_socket_group(group, reason)
```

`set_socket_interests` returns proposal admission only; it does not claim deferred
codec/encoding success. Engine invokes the encoder after restoring registry and
Venue borrows, then commits the complete transaction or reports the proposal as
`InterestsCommitted`/`InterestsRefused`; refusal retains old desired/released
state. Pure state tests use a synchronous encoded transaction input.

`connect_socket` is permitted initially or after an engine retry grant, never
from an arbitrary duplicate open callback. Only the engine holds Supervisor,
auth/open state, login retry timer and paced batch timers. Native connection
slots retain a FeedId and codec configuration, not a parallel current SocketId,
epoch/open/auth bool, Supervisor or queue policy. All ordinary `Cx.send/close`
operations on managed sockets enter the same admission/retirement path.

Engine integration is limited to Io registry/effect queue, Cx facade, WS ingress,
managed timer dispatch and final WS action admission. Reuse existing physical
close/signing cancellation; do not alter signing_dispatch Pending/finish/release.
Before any signing receipt callback or queued WS release, recheck the live
physical epoch. Drop retired ordinary queued actions too. Subscription frames
stay in the engine queue until final drain, tagged by epoch/interest revision;
only then publish plain WsSend actions. Do not emit or mark coverage while an
existing signed upgrade/auth barrier still holds that socket: keep frames in the
registry until it clears. Thus no signing-side-map or managed frame loses its
revision metadata inside a held plain-action queue. Released means final engine
drain handoff, not venue ACK or proof of physical network delivery. The host wire
has no new WS action type.

Each event/drain consumes lifecycle effects synchronously before replacement
output is exposed. Retirement commits first, cancels timers/frames/signing, then
delivers required invalidation; reconnect callbacks can only consume a retry
grant. A bounded effect loop rejects codec-driven recursive lifecycle churn.
Bounds to freeze: at most 512 logical feeds, 8192 interests/feed, 4096 queued
frames/feed, 64 KiB/frame, 4 MiB total queued WS bytes/engine and 8 MiB total
retained interest key/argument bytes/engine; duplicate/conflicting
interest keys and non-finite deadlines are typed refusals. Counters use checked
add; exhaustion never aliases a live epoch. A feed with suppressed public output
creates no live epoch, watchdog or replay.

The engine enters managed WS mode when a family registers its feeds. Only those
registered current sockets are admitted in that engine; families registering no
managed feeds remain on the unchanged legacy path. Both reference families adopt
all their socket roles together, so mixed unmanaged escape sockets are refused.
This avoids an unbounded retired-ID tombstone set and avoids dropping every
legacy venue's events. Disposal cancels the registry before releasing host work.

Original-code controls planned first: OKX duplicate open/accepted ACK cannot
repeat login/subscriptions/resync; late auth acceptance after its absolute bound
cannot rescue the socket. Bitunix stale/duplicate close/open/traffic cannot alter
engine health, and an expired current feed cannot be rescued by traffic before
delayed timer delivery. Existing malformed/explicit refusal/epoch isolation and
first-pong suites remain compatibility controls. RED means a runtime assertion
on unchanged production, never a compile failure.

### Shared allocation prerequisite (owner-approved refinement)

The current `next_engine_id` wraps u32 across physical sockets, timers and HTTP.
A local epoch cannot reject a delayed host event once that physical ID is reused.
Owner approved widening all shared IDs/counter to u64 capped at `2^53 - 1`,
preserving JS Number precision and retaining checked fail-closed exhaustion as a
backstop. No per-venue counter or tombstone workaround. Coordinate with the
RequestContext owner before retyping the shared Io helper; its atomic managed
reservations and prior-send outcome evidence must survive.

Proposed shared allocator: `try_reserve_engine_ids(&mut next: u64, count: u64)`
returns a checked non-reused range or typed Capacity, committing the counter only
on complete success. Existing infallible single-ID allocation calls the same
primitive and fails closed at the safe-integer boundary; new managed admission
uses the typed reservation before side effects. No dummy ID on failure. WASM
ingress remains a JS Number via validated finite safe-integer f64 conversion,
not wasm-bindgen u64 BigInt; Rust/native/JSON paths retain the full integer. Audit
all host encoders, timers, signing/request receipts and narrowing casts before
source changes. Owner sequencing is explicit: audit/plan now; allocator source
only after actual RequestContext #1741, as its own small shared PR. Socket work
may continue in the independent state/effect policy; production adoption depends
on that actual allocator prerequisite, not a local counter workaround. Tests must cross the old u32 limit, preserve distinct socket and
timer IDs after many allocations, reject stale retired ingress and reject final
capacity/invalid-number input without reuse or partial reservation.

### Original production RED receipt

Against unchanged production on actual #1764 plus this plan, three OKX runtime
assertions fail: duplicate open emits a second login/ping; duplicate accepted
login repeats subscription frames; late accepted login rescues a phase at its
auth deadline before timer delivery. Three Bitunix runtime assertions fail:
duplicate retired close increments health, retired open increments health, and
a late pong rescues the first-pong-activated idle deadline before timer delivery.
Logs: `/tmp/codex-socket-okx-original-red.log` and
`/tmp/codex-socket-bitunix-original-red.log`. Both suites compile and each runs
three failing assertions; no compile failure is counted as a RED. These are
local test-only controls until the shared core and adoptions make them GREEN.

## Independent state component checkpoint

`engine/src/socket_lifecycle.rs` now contains engine-private state authority for
epoch admission, typed auth transitions, terminal group stop, retirement ACK
quarantine, and desired/queued/released subscription transactions. It is compiled
through `engine/tests/socket_lifecycle_state.rs` only at this intermediate stage;
there is no runtime export/driver or venue adoption yet. It neither duplicates
Supervisor timing/backoff nor supplies a venue-side lifecycle helper.

Both source reviews are CLEAR after runtime reproductions and fixes for three
review findings: terminal group stops had left inactive siblings retryable;
aggregate interest bounds had been checked after encoding; repeated positive
auth after a failure had been hidden from retry cleanup. The corrected component
passes 22 focused controls, strict Clippy and fmt/bans. Existing engine/Binance
library suites pass. The emitter checks queue bytes/count incrementally and
keeps a sticky refusal even if a codec ignores its returned error. Retained-byte
reservation includes future released copies across all feeds before encoding.

The six OKX/Bitunix original-production tests remain intentionally RED until the
engine driver and adapters are wired; this checkpoint does not claim otherwise.
Next: driver effects/timer/signing/health admission using the existing ID types,
then both adoptions. No allocator source is changed here; actual #1741 and the
separate shared widening PR remain required before production merge.

## Driver supervision phase

Extend the existing Supervisor with a read-only `expired(now)` query and explicit
idle activation. The engine will query expiry before ingress, using the exact
same predicate as timer delivery; it must not maintain a second deadline model.
Idle activation defaults to enabled, preserving all existing callers. A managed
Bitunix private feed disables it until a real pong, then enables it with the
current raw timestamp. Disabling idle does not disable connect/auth bounds or
ping cadence. Enabling an already enabled guard is idempotent and cannot extend
its deadline. New unit controls cover exact expiry, non-consuming queries,
first-pong activation, and ping/auth independence. This phase does not wire
managed ingress or turn the six original venue RED tests green by itself.

The Supervisor hooks pass all 14 focused socket unit tests, including the four
new expiry/activation controls, and strict all-target engine Clippy. Correctness
and adversarial reviews are CLEAR: default-enabled idle keeps legacy deadline
priority, watchdog cadence, ping and backoff behavior unchanged. Runtime driver
integration must disable idle on **every** new first-pong epoch and check expiry
before activation/traffic/auth. Bitunix current text must still reach the stable
backoff observation even before its first pong; idle activation and stability
proof remain independent. The six venue runtime RED controls remain unresolved
until driver/adoption work; this checkpoint makes no runtime closure claim.

## Supervised transition driver phase

The next internal component couples Lifecycle to the existing Supervisor using
real Cx timers, before adding engine ingress hooks. It owns per-feed Supervisor
and coalesced login-retry timers; transitions return bounded, owned effects for
Engine to deliver after restoring the registry borrow. No venue callback runs
while this component is detached. Retirement revokes the epoch and cancels its
timers before any effect is exposed. A required invalidation acknowledgment is
needed before the Supervisor can schedule reconnect; terminal groups cancel
inactive sibling timers too and never regain a grant. Timer events are scoped
and retired ones cannot reach venue callbacks. The component remains private to
the engine and is staged under the focused native harness until engine wiring.

Observe-only auth has an explicit `Authenticated { first }` metadata effect
distinct from Ready. Bitunix accepts native server-push before login ACK and has
no private subscription batch. Its existing on-open path starts account recovery
when positions are loaded; the ACK only cancels the login retry. Adoption must
keep that recovery on open, never add ACK-triggered resync or gate it on auth.
Duplicate positives cancel a pending same-socket login retry without granting a
second Ready. Expiry is checked before both ingress and auth/traffic;
current text may reset Bitunix's stable backoff before its first pong, while idle
activation still requires the pong. This is codec-independent policy, tested via
a probe using actual Engine/Cx timers; no allocator or HTTP dispatch source edit
belongs to this phase.

The driver does not invoke public Cx.close recursively. Retirement effects reach
Engine after state/timer revocation; Engine first uses an internal raw transport
close/signing-cancel primitive, then invokes native invalidation, then submits
the acknowledgment. Direct public close enters retirement exactly once. The
staged probe uses ordinary close only because its Io has no managed registry.

The staged transition driver passes 31 controls (22 state + nine supervised
Engine/Cx probes), 397 library tests across engine/OKX/Bitunix/Binance, strict
all-target Clippy for those four crates, fmt and banned-pattern checks. Both
final correctness/adversarial reviews are CLEAR at driver SHA
`8c1ab05f66383f730efa4c9f1b6067fc6b17fc238f93861933c22ca7ad0cbf64`.
Review found and corrected non-representable clock handling: a shared preflight
rejects NaN, infinities and non-advancing timer additions before physical
allocation or epoch/auth/timer mutation. It validates representability only;
Supervisor still owns expiry and backoff. `timer_admissible` must be queried by
Engine **before** removing a live managed timer from Io; the staged probe does
not prove that eventual engine boundary yet. Raw-close/signing cancellation,
health/event admission, subscription drains and the two actual adoptions remain
unwired; the six original venue RED tests are still expected to fail. No live
runtime, allocator, hosted wire, replay or cutover-readiness claim is made here.

## Physical-close seam before runtime wiring

Extract the existing Cx.close transport/signing-cancellation body into a private
engine-module method. Public close delegates unchanged until managed routing is
wired. The future retirement effect consumer can then use the private method
without reentering lifecycle retirement. This changes no close action, timer,
health or public suppression behavior and touches no allocator/HTTP dispatch
region. Verify the existing Supervisor controls and review the exact extraction.

The physical-close extraction passes the existing 14 Supervisor controls and
strict engine library Clippy. Both independent reviews verify its private body
is byte-identical to the prior public body, public close delegates once, and no
reverse call or recursion exists. Signing-cancel/ClearTimer, health and WsClose
ordering and suppressed-public behavior are preserved. Runtime managed effect
routing remains the next phase.

# Option 1 variant scope

This branch prepares OKX-only adoption on actual #1820; it is not a merge
selection. The independent plan in RUST_SOCKET_OKX_ONLY_VARIANT.md governs this
variant. Bitunix stays exactly on the actual landing's legacy path and remains
in the temporary socket inventory. The original two-family plan, failed
measurements and source checkpoints below are retained as historical records,
not assertions that Bitunix has migrated on this branch. No gate threshold or
build profile changes.

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

## Live Engine wiring and reference adoption phase

Export the codec-facing lifecycle types and move the staged component tests onto
those actual modules. Io owns Driver, a managed-mode latch, bounded deferred
native effects/interests, and proposal IDs. Cx registers feeds, admits connection
before physical allocation, proposes complete interests, queries epoch/auth and
retires feeds. Scope driver mutations through a detached borrow, restore it
before native callbacks, and keep managed mode latched so an empty/detached
registry never becomes a legacy bypass. No allocator type/helper changes.

Engine admits WS open/text/binary/close before health or native mutation. It
checks owned timer admission before Io removal; unowned account/poll timers keep
the existing path. Typed native auth codecs run after current-epoch admission;
opened/ready/auth/failure/ping/retry/retire/reconnect effects run only after
registry restoration. Retirement first closes/cancels through the private raw
seam, then invalidates native owned state, then acknowledges retry. Callback
failure quarantines. Native group stops revoke all siblings first. Bound effect
processing and reject recursive work on exhaustion. Subscription proposals are
bounded and transactional; Engine invokes the pure encoder after borrow restore.
Final drain alone updates released coverage, after cadence, current epoch,
Supervisor expiry and signing barriers. Recheck ordinary and signed WS outbound
and receipt publication; parked retired signing cannot reconnect or emit auth.

Adopt every OKX public/private/business and Bitunix private/futures/ticker role
together; remove their local Supervisor/open/auth/queue/login-retry ownership,
retain native codecs and metadata only. Preserve on-open Bitunix recovery,
first-pong idle, current-text stability, native 20/250ms batches, OKX terminal
credential codes and private/business group, pong-only liveness, native signing
and store/history/freshness fences. The six original runtime REDs must turn
green through the actual Engine hooks, alongside compatibility and parked-signer
controls. Add caller/audit/quality rows, independent correctness/adversarial
reviews and draft PR after focused native/strict/SDK checks. Keep #1767 watch;
#1763 waits actual queued inputs for its next CI/proof. Production allocator
widening remains a separate post-#1741 PR.

### Live adoption review and bounded cleanup

The shared Engine path now handles open, text, binary, close, signing completion,
queued outbound, and owned timers; both reference codecs use it. Managed events
retain the existing event prelude/epilogue and held-cancellation processing.
Unexpected host closes grant reconnect health evidence once; explicit closes
remain deliberate. The common stop operation revokes Initial/Waiting/Granted
feeds as well as active epochs. Callback and admission errors emit bounded coded
errors; failed invalidation never grants retry. Effect-capacity failure preserves
already revoked cleanup ownership before stopping every remaining active feed.

Engine probes cover invalid-time timer retention, health reset, retry revocation,
queued reconnect denial, overflow cleanup, refused callbacks, native forced book
repair, and parked/ready signer retirement. Existing six original runtime RED
cases are retained. Compatibility tests must use current epochs: serial retry
checks now isolate each Bitunix feed, and a REST-recovery test obtains a fresh
connection rather than delivering an open after its original connect timeout.
No deadline is extended to accommodate these tests. Native payloads, request
builders, account/read fences, entropy, and credential custody remain unchanged.

This draft does not widen physical SocketId/TimerId/RequestId or Io.next.
Production merge is blocked on the separate safe-integer allocator PR after
actual RequestContext #1741, followed by real integration, final CI/replay proof
and independent reviews. Other venue families retain their legacy lifecycle
until adopted; no universal socket-standardisation or cutover closure claim.

Live focused verification: 1,180 native tests pass across Engine, Binance, OKX
and Bitunix, including all six original runtime RED controls, ten shared Engine
probes, exact native public batching/ping bytes and all financial reducer
assertions. Independent correctness review is CLEAR for Engine routing, shared
stop/overflow cleanup, native codecs and compatibility fixture updates. Strict
checks, adversarial review, actual dependency integration and CI remain gates;
this result does not assert WASM/replay or allocator completion.

The final adversarial pass additionally found standalone deferred-frame wakeup:
`has_actions` now delegates one-shot eligibility to Driver's existing valid-time
policy. Current ready/nonbarrier frames wake once; invalid raw clocks retain
ownership without empty-drain spinning and valid time later releases once.
OKX auth codecs require object-shaped evidence before typed decode; array-shaped
Serde structs cannot imply acceptance or malformed-login recovery. Borrowed
EventName/TopicArgs avoid dynamic JSON object graphs. The unchanged Bitunix login
error literal stays in its original native account module, with no signing
allowance addition or credential-policy change. Both independent final reviews
are CLEAR; strict four-crate all-target Clippy, fmt, bans and diff checks pass.
Final 449 focused unit/runtime controls pass; the complete four-crate native
rerun finished with 1,182 tests passing. Actual hosted-presentation #1763 at
d05c5ede4b86409e1a8d9798e64b79b1c1c888f4 is genuinely integrated; all owned
source/test blobs and the complete incoming unowned tree are preserved. Four
shared doc/index append conflicts retained incoming first and both blocks
verbatim once under standing authorization; no source/test conflict occurred.
Integrated focused verification passes 583 tests, with strict four-crate
all-target Clippy, bans and fmt checks. A final OKX Ready-path correction
propagates synchronous interest admission failure before account refresh;
catalogue/command callers retain the common coded-error-and-stop wrapper.
Independent review is CLEAR, and 42 OKX account/socket controls plus strict
all-target Clippy pass after that correction. Draft CI remains pending.

## First CI replay corrections

Exact draft `7f8c0eb14fb` / Rust39841774617 failed the OKX and Bitunix public
verification gates. These are not retry/waiver candidates. Independent source
review identifies repeated full-catalog scans inside OKX's subscription sort
comparator, and new empty BOOK publication when retiring an OKX subscription
that never received a snapshot. The latter matches the EEA checkpoint-1 additions.
Bitunix steady CPU also regressed; an idle engine drain currently allocates a
feed vector and traverses all authorities despite having no pending frames.

1. Add an original-RED native market-frame test: retirement before first depth
   must not materialize an empty consumer book; retirement of an actually loaded
   book must still clear it immediately. Use existing sequence/book state, no new
   venue-owned epoch/auth flags. Preserve gap recovery and published empty clears.
2. Precompute OKX native market/underlying ordinals once per encoding and each
   interest's rank once before sorting. Keep exact original stable topic and
   market order, first-underlying ownership, 100-argument batches and singleton
   book frames. Add a multi-market/repeated-underlying order oracle, no benchmark.
3. Add a shared pending-work fast path to skip drain allocations when neither
   persistent frames nor epoch-owned one-shots exist. This is presence only;
   every actual release still uses the same current-epoch/time/auth/signing
   barriers. Retain existing expired/stale/paused/paced/one-shot tests and add a
   no-pending-to-later-pending transition control if not already covered.
4. Run focused engine/OKX/Bitunix/Binance checks, strict lint/fmt/bans and both
   independent reviews. Real-integrate actual1769 before new CI. Public CPU and
   complete replay evidence remain CI-only; no local replay or timing claims.

Architecture gate: corrections to the existing shared socket choke point and
its pure native wire adapter, not a venue-local authority, clock or allocator.
Safe-ID widening still waits for actual #1741 as a separate shared PR. The draft
cannot merge until that dependency and fresh CI/proof pass.

The first correction is reproduced: retirement before first depth was assertion
RED while the existing published-book clear control was GREEN. Both now pass.
The guard uses existing sequence/level evidence, preserving loaded empty snapshots
and nonempty gap recovery. The ordinal cache passes a 128-market reversed-input
oracle with repeated underlyings, exact native topic/market order, 100-argument
chunks and singleton books. Engine idle paths now test pending-work presence
before allocating feed vectors or scanning release authorities; actual release
checks are unchanged. Both independent correctness and adversarial reviews are
CLEAR, including idle-to-pending repair/one-shot coverage. The 408-engine / 7-OKX
unit run passes; the complete four-crate native rerun and fresh CI are pending.
No performance or complete replay equality is inferred from these local checks.

Correction verification: all 1,195 engine/OKX/Bitunix/Binance native tests pass,
with strict four-crate all-target Clippy, fmt, bans and diff checks. Actual row
reporting #1769 at90dc6637d6e is genuinely integrated atb2a390201fe. Independent
preservation confirms every owned Rust source/test blob matches reviewedc71,
every unowned path matches landing, and all three append-conflict doc blocks are
retained incoming first/verbatim once. Integrated engine/OKX/Bitunix unit checks
pass436 tests. Fresh CI must remeasure both public families and authenticate
complete replay output; no first-run failure is dismissed or normalized away.

## Actual RequestContext integration and retained-interest correction

Actual #1741 `b2fe97324763e1e2702c69ac5b839da37102928b` is merged at
`c3b0646d35b`. The three engine ingress/finalization conflict resolutions were
explicitly approved on #1771; both independent ordering reviews confirm every
HTTP admission, delivery scope, managed signing and settlement rail is retained.
A new original-RED control proves socket callbacks cannot borrow an outer HTTP
delivery; the shared callback boundary clears/restores it on return. The four
engine/OKX/Bitunix/Binance crates pass 1,343 native tests after integration.

The previous CI correction fixed CPU and checkpoint book regressions and passes
Bitunix. Its remaining unexpected gate is OKX EEA-XPerp retained memory:
Rust 8,004,952 versus TS 7,205,904 bytes. Shared interest maps deep-copy immutable
records and transiently clone the released map. This phase shares immutable
`Arc<Interest>` records between desired/released maps, reuses unchanged desired
records, and moves released state only after every validation/encoding/coverage
check succeeds. The public Interest/encoder API, exact frames and release order,
epoch/auth/ACK/repair behavior and logical byte budgets remain unchanged.
Original-RED allocation-identity controls accompany same-key argument replacement,
failed-transaction atomicity and reconnect replay checks. Fresh CI establishes
actual retained memory; pointer sharing alone makes no numeric savings claim.
Independent plan review is CLEAR; nonempty payloads pin actual allocations,
and logical double-copy reservations remain unchanged despite physical sharing.

Shared-storage controls reproduce three original allocation-identity assertion
failures with 23 compatibility passes, then all 26 lifecycle controls pass. Both
independent implementation reviews are CLEAR: logical budgets and original
released arguments survive sharing, failure remains atomic, and the public
encoder receives the same values. Four-crate native unit checks, strict
all-target Clippy, formatting and bans pass on the corrected source. Fresh CI
will measure memory and replay the full Binance dependency; no result from the
previous socket head is relabeled. Safe-ID widening remains a separate PR.

## Compiled ownership boundary after fresh gate findings

Correction head `4382c6fc326` still fails OKX EEA retained memory (Rust 7,547,104
versus TS 7,202,960), Bitunix steady CPU (108.156 versus 114.042 ms), and Aster
module size (1,370,122 versus 1,343,488 bytes). The ledger error follows the size
refusal; it is not an independent replay failure. No rerun or waiver is taken.

Read-only audit confirms the managed socket Driver, storage/drop glue and generic
Engine hooks are compiled into unadopted standalone families. Introduce one engine
`managed-sockets` feature, enabled by the two adopted OKX/Bitunix dependencies.
Gate component modules, Io storage/default, managed Cx API and Venue extensions,
and engine routing/signing/queue/timer hooks together. The feature-off engine keeps
its exact legacy path, with no managed driver/storage/drop code. Existing HTTP
RequestContext, signing and native legacy socket policies remain unchanged.
The combined registry naturally unifies the feature when both reference venues
are present. This follows existing shared engine feature boundaries rather than
adding per-venue runtime lifecycle policy.

Verify focused engine checks with the feature off and on, both adopted venues
and Binance, all-target strict Clippy, then fresh CI for standalone Aster size,
combined host fixtures and the full replay. Retained-memory representation and
hot-path work remain separate measured failures to address; the feature boundary
alone makes no claim to repair either.

The feature is temporary, not a permanent opt-out. End state: every socket
family uses this shared core, the legacy inventory is empty, then the feature
and feature-off compatibility paths are deleted. Remaining family/site records
are checked against source and must only shrink against existing history and
the immutable PR base, just like the legacy-request inventory. Aster's later
adoption must shrink code to fit the unchanged budget; no budget increase is
a migration mechanism. Managed venue wrappers are named `ensure_feed` so the
conservative legacy `connect` reference scanner needs no receiver exemption.

Remaining legacy WASM families: apexomni, asterdex, backpack, binance, bingx,
bitget, bitrue, blofin, bybit, coinbase, cryptocom, grvt, hyperliquid, kraken,
kucoin, kucoinspot, lighter, mexc, orderly, paradex, phemex, polymarket, txflow,
weex and woo. Kraken Spot shares the Kraken family; KuCoin Spot is a separate
family within the KuCoin crate. BreakoutProp has no socket path.

The checked inventories are
[socket-legacy-families.json](../../sdk-rs/scripts/socket-legacy-families.json)
and [socket-legacy-sites.json](../../sdk-rs/scripts/socket-legacy-sites.json).
The seed has 25 families and 90 conservative member/qualified/helper references
in 48 files, including the 47 concrete Cx connect sites from the audit. The
checker reuses the request ratchet's lexical and immutable-base authority but
none of its HTTP bootstrap exceptions. Alias/UFCS/member references and macro
escapes are covered; source counts must equal the inventory and both site/family
maps must be subsets of every available historical/base map. Empty legacy
family inventory requires deletion of the temporary engine feature.

## Canonical replay storage follow-up

The retained-memory and steady-CPU findings above remain real. The temporary
feature boundary has no fresh measurement yet. First reduce avoidable socket
interest allocation in the shared component, without changing venue codecs:

1. Keep the existing `Vec<Interest>` proposal and `&[Interest]` encoder contracts.
   Canonicalize external proposals to validated `BTreeMap<String, Arc<Interest>>`
   once, then route external replacement, reconnect replay and repair through one
   transactional preparation/coverage/budget/commit implementation. Add an
   internal replay entrypoint that clones canonical Arc handles instead of
   deep-copying every desired key/argument into a throwaway proposal. Repair
   likewise uses canonical rows. Reconnect still starts from empty released
   state; replay increments the same revision and preserves exact codec frames.
2. Final batch release consumes its owned coverage keys into the released map
   instead of cloning each key immediately before dropping the queued batch.
   Keep epoch, readiness, signer barrier, revision and desired-row checks in the
   existing final-release path; staging still grants no release progress.
3. Original assertion-RED allocation controls prove nonempty coverage-key storage
   is transferred at final release. Add replay/repair compatibility and failure
   atomicity controls: same desired Arc identities, exact unsubscribe/subscribe
   order, rejected encoder/coverage leaves old state and queued frames unchanged,
   stale epoch/barrier cannot consume keys. Logical double-copy reservation and
   all per-feed/aggregate input/frame/queue bounds remain unchanged.
4. Independent plan and source reviews, lifecycle/driver/engine ordering controls,
   focused engine/OKX/Bitunix/Binance tests, strict Clippy/fmt/bans. Fresh CI alone
   decides retained-memory, Bitunix CPU and size; no local timing or claimed
   numeric savings. CPU ingress/runtime-index changes remain a separate phase,
   not bundled as an unmeasured behavioral shortcut.

Architecture gate: canonical ownership and release allocation are shared engine
socket concerns. No OKX-specific storage path, budget increase, allocator change,
HTTP delivery/admission change or native codec rewrite belongs in this phase.

Checkpoint: committed original test `e176897f616` reproduces the nonempty-key
allocation assertion failure with 27 compatibility passes. Canonical replay and
repair now retain Arc handles through the same transaction, and final release
moves owned coverage keys. Thirty lifecycle controls, including current external
queue refusal, post-emission encoder failure, partial-drain preservation and
repair ordering, pass; four-crate unit tests pass 585, strict all-target Clippy
and engine fmt pass. Both independent source reviews are CLEAR on lifecycle
SHA256 `598cf26427eb0aa61530aa16098460f9bb4475feca28c32b4f6d450bcba2a10d`
and engine-socket SHA256
`ca1ea26a50564c1b72081a3e9ecb7213ead269823adb82a502e47fc1f36fcf57`.
The full ban check correctly refuses the stale Hyperliquid legacy inventory on
this older socket branch against actual #1765; no exemption or inventory change
is made here. Integrate current landing/shared prerequisites, rerun that gate,
then obtain fresh CI before any readiness or memory/CPU/size claim. The existing
numeric failures and separate safe-ID prerequisite remain open.


## Measured failure follow-up on frozen SafeIDs

The owner authorized exact-head stacking on SafeIDs #1789
`076af5e782cf6588521543aada2bb98ba49bd6e6`. Integration `0b2b3be7aaf`
retains incoming resource and HTTP rails. Two conflicts were resolved only after
explicit owner disposition: per-action safe validation precedes the complete
socket gate/callback block; drain validates before socket work, revalidates and
retains the original socket fence/frame order plus final validation. Two
independent ordering/full-tree reviews are CLEAR. A new original assertion RED
proved a callback catching last-ID exhaustion allowed a second callback (2 vs 1);
central postcallback fatal/output fences make it GREEN. All 585 all-feature
engine controls and 614 four-crate unit controls pass, including HTTP-delivery
isolation. SafeIDs CI is all43 green; its actual-WASM ingress fixture passes98.
These are source/checkpoint facts, not final socket performance or merge proof.

Historical failed source `5f901f18027516d7611b9fb42d8a84203affc8c3`, run
39862993196 attempt1, remains the measured RED. Exact strict ZIPs were freshly
downloaded for read-only profiling and digest verified: OKX artifact12463171541
SHA25652d2a713149741c37bf57d598c6ba3a065fb5b56d705ec0b14ec4c61a97948c4;
Bitunix12462836721 SHA25634ce6d44f5e326cd35eb37952c30117074887ff9e67a92bd6f5c20da4ff3a488.
OKX EEA median retained bytes are7542392 vs TS7203920 and target6483528;
Rust component medians: WASM4784128, JS heap2439896, buffers318368. Bitunix
steady CPU is193.874ms vs TS202.186 and target181.9674ms; memory passes.
No local performance execution, rerun, exception or altered threshold is used.

Concrete shared changes, plan and audit before source:

1. Replace owned subscription key/argument and coverage strings with immutable
   shared text (`Arc<str>`), keeping public string bytes/order unchanged. The
   canonical desired/released map keys reference the same key storage; identical
   key/native argument text can share one allocation through a generic Interest
   constructor/normalizer. Encoder scratch clones immutable handles, not payload
   strings. Existing per-feed/aggregate logical byte reservations, count/frame
   bounds, failure atomicity, release-only evidence and revision/epoch barriers
   stay unchanged. Adapt the two native codecs only to supply this representation.
   Original assertion-RED controls check desired key/argument aliasing, map-key
   aliasing and final released/coverage storage identity; retain exact native wire,
   replay/repair, partial-release and refused encoder/coverage controls.
2. Add one lifecycle-owned index of current physical sockets to epochs. Begin
   inserts only after full preflight; retire removes before callbacks/retry;
   stop/permanent refusal use the same path. Runtime storage follows the already
   append-only FeedId index. Driver ingress resolves the current epoch/runtime
   once, reads its own Supervisor expiry directly and retains fresh time/epoch/
   phase checks at BOTH ingress and control entry. No venue bypass/admission cache
   or independent deadline is added. Keep stale/unknown sockets denied and empty
   lifecycle Legacy behavior unchanged. Native controls cover multiple feeds,
   healthy/expired neighbors, retire/reconnect, invalid time, duplicate sockets
   and no resurrection; immutable current index remains bounded by MAX_FEEDS.
3. Re-run independent source/adversarial/ordering reviews, engine/OKX/Bitunix/
   Binance focused tests, strict all-target Clippy/fmt/bans, then push #1771 for
   fresh full CI. CI decides both actual targets and all fixed budgets; no metric
   claim follows from pointer tests/source inspection. The official gates and
   raw output comparison are unchanged; matching SafeIDs actual ancestry and
   subsequent required input integration remain merge prerequisites.

Architecture gate: shared lifecycle storage and admission lookup are engine choke
points, consumed by two different native codecs. No venue policy, allocator,
HTTP release/authority, retry/auth semantics, budget or external wire change.

### Shared text storage checkpoint

Four original assertion-RED allocation controls are GREEN after canonical
`Arc<str>` sharing across desired/released map keys, native arguments, coverage
and encoder scratch. Logical byte charges and frame/batch/coverage bounds are
unchanged; identical key/argument normalization happens only after preflight.
OKX and Bitunix retain the existing exact wire and ordering assertions. All
1,384 four-crate all-target tests and strict all-feature/all-target Clippy pass;
both independent correctness and adversarial reviews are CLEAR. This checkpoint
makes no measured memory or CPU claim; the indexed lookup phase and fresh CI
measurements remain required.

### Indexed current-socket lookup checkpoint

Lifecycle owns one bounded current-socket-to-Epoch index. Begin inserts after
all existing preflight checks; central retirement removes ownership before any
native callback. Lookup revalidates the full Epoch against the current feed.
Driver runtime indexing follows append-only FeedId registration, and ingress
reads only that runtime's Supervisor. Fresh representable-time, phase, epoch and
expiry checks remain at ingress and again at control handling; no admission is
cached. Three new compatibility controls pass on the original linear code;
the added 512-feed index control covers all retirement paths and bounded storage.
All 1,388 four-crate all-target tests and strict all-feature/all-target Clippy
pass; both independent correctness/adversarial reviews are CLEAR. The ban check
correctly refuses the older ApeX legacy-request inventory against the newly
landed #1772; actual integration is required before CI, with no exemption or
manual inventory growth. Performance remains unmeasured on this checkpoint.

### Exact 03006 CI measurement and remaining shared hot path

Run39887733824/head03006d1224ad427c768a00367c1daaffb796f9ba attempt1
measures the original EEA xperp memory objective GREEN:5932056B vs TS7203768,
target6483391.2B. The exact OKX strict artifact and Bitunix diagnostic are bound
by fresh run/head/artifact/digest checks in
`/tmp/codex-socket-03006-metrics/report.json`; this is diagnostic evidence, not
final replay/package proof. Original performance failures remain historical.
Current CPU failures are real: Bitunix106.98ms vs TS106.083/target95.4747;
OKX inverse197.401 vs TS213.743/target192.3687 and EEA178.778 vs
TS196.753/target177.0777. No retry or timing exception is used.

Next narrow shared phase, audit before source: make idle pending-frame presence
constant-time with one Lifecycle-owned total queued-batch count. Preserve exact
per-feed queue identity/revisions/order and all admission/release checks. Update
only at successful transactional queue replacement, actual pop (including a
rejected stale revision), and central queue clearing used by begin/retire/group/
stop. Failed encoding/coverage/budget/epoch/signer-barrier operations leave it
unchanged. The count is the exact sum of queue lengths, not send authority or a
new budget. Keep external one-shot accounting separate. Existing per-feed and
aggregate byte/frame bounds remain unchanged. Add mutation-path invariants plus
multi-feed partial release/replacement/refused-transaction and retirement controls.

Use an ordered current-socket map rather than a fresh randomized hash-map default
on every temporary driver move. It preserves the same private exact Epoch index,
bounded cardinality, lookup validation and retirement timing; no identity/policy
change. This avoids unnecessary per-message hasher initialization and uses the
already-required tree representation. Re-run both independent reviews, focused
four-crate tests/strict/fmt/bans and fresh CI. All CPU objectives remain real
failures until that CI passes. The frozen03006 evidence stays historical on any
new source head; no artifact relabel or automatic retry is authorized here.

The constant-time presence phase passes1,391 four-crate all-target controls,
strict all-feature/all-target Clippy, fmt, bans and both final independent reviews.
The two pre-source compatibility controls also pass on the old scan path. The
ordered map uses a private lossless u64 key, preserving public SocketId traits.
Exact-count controls independently sum queues across failed transactions,
barriers, partial release, stale pops, reconnection, group double clear and
inactive stop_all. Inactive stop_all retains its existing queue behavior rather
than inventing cleanup; its count remains accurate. No measured CPU result is
claimed for this checkpoint.

### Remaining Bitunix CPU after exact 9a1 measurement

Run39890112113/head9a1d0dbddc1 attempt1 passes every OKX public objective,
including xperp retained memory. Bitunix steadyCPU remains a real failure:
Rust208.221ms vs TS213.992/target192.5928. Exact diagnostic archives/report
remain `/tmp/codex-socket-9a1-metrics/report.json`, not final proof. No retry.

Source hot-path audit finds a remaining native feed-vector scan solely to return
Control::Other on every Bitunix public/business frame; those feeds have no native
control messages, while private login/pong evidence remains significant. Next
shared API: explicit ControlPolicy::{Native,None} on FeedPolicy. None is valid
only with AuthMode::None and TrafficPolicy::AnyText. Engine bypasses only the
pure native classification callback for that declaration, substitutes Other,
and still runs the unchanged fresh Driver.control/epoch/expiry/traffic path and
native message callback. OKX declares Native; Bitunix declares Native for private
and None for control-free public/business. This is immutable codec data, not a
venue shortcut or cached send/auth admission. Other policies reject the None
combination before feed/runtime registration. Pin default/native classification,
zero classifier calls for None, native message delivery and unchanged retirement
at the exact idle bound, plus both family wire/auth controls.

The fixed-policy representability audit also repeats the same interval list on
every ingress/control check. Precompute only its shortest/longest positive delay
in the driver Runtime at validated registration. Fresh raw-time admission checks
both extremes at every existing boundary: IEEE monotonic addition proves every
intermediate positive interval advances and remains finite iff both extremes do.
Keep independent old-formula equivalence controls over finite/nonfinite, negative,
subnormal, huge and rounding-boundary values; preserve all invalid-time and
stale-timer controls. Supervisor remains the only deadline/backoff authority.
No raw-now caching, deadline change, allocator/HTTP edit, frame policy or budget
change. Review concrete API/tests before source; focused native/strict/fmt/bans,
independent reviews and fresh CI must establish the remaining CPU objective.

PhaseD source passes1,433 four-crate all-target controls, strict all-feature/
all-target Clippy, fmt, diff and bans. Both independent correctness/adversarial
source/API/math/ordering reviews are CLEAR. The original-formula oracle checks
72 policy variants across77 raw-time boundaries per variant; classifier tests
pin Native/default behavior, no-classifier native delivery, invalid raw time,
exact idle expiry and retired ingress. Contradictory None policies refuse before
registration. This is native/source verification only; remaining Bitunix CPU is
still a real blocker until fresh CI measurement passes.

### Phase E — one current-socket lookup per fresh admission

The unchanged03a single Bitunix retry remains a real CPU failure (200.936ms
Rust,193.727ms TS,174.3543ms target); no additional retry is allowed. Static
hot-path audit finds each Driver ingress looks up the exact current physical
epoch for fresh time/expiry validation, then Lifecycle.admit repeats the same
bounded-index lookup immediately. Native control correctly re-enters ingress,
so an ordinary text event repeats four map lookups even with no intervening
socket mutation. Keep both fresh ingress/control admission checks and every
Supervisor expiry/time check; remove only the duplicate lookup inside each.

Add a crate-private Lifecycle.admit_checked(socket, ingress, check) boundary.
It resolves one exact current epoch, invokes a synchronous FnOnce(Epoch) checked
fresh raw-time/expiry query from the Driver's immutable runtimes, then retains
the exact existing phase/expiry/retirement order. Invalid time still denies;
unknown/retired sockets never invoke the query. Driver supplies the existing
TimeBounds.valid and Supervisor.expired at the same cx.now. There is no cached
clock/admission token, new permission, venue policy or public API. The existing
static-expiry test helper can remain test-only and delegate to this boundary.

First commit assertion-RED controls with test-only lookup accounting: current
message admission and the independent fresh control boundary each perform one
index query, rather than two. Pin accepted delivery/idle receipt, exact expiry,
invalid raw time, retired socket refusal and repeated-generation behavior.
Keep native control revalidation, all OKX/Bitunix wire/auth tests and SafeIDs
fatal/HTTP-delivery isolation tests. Production changes are confined to the
shared socket driver/lifecycle; no venue, Engine HTTP, allocator, host, harness,
budget or timing-threshold edit. Review the concrete API/order before source,
run focused four-crate native/strict checks and independent source reviews,
then measure the resulting exact head in CI. A reduced lookup count is not a
CPU-pass claim.

PhaseE source verification passes 1,446 engine/OKX/Bitunix/Binance all-target
cases and67 targeted all-feature socket controls, strict touched-crate Clippy,
fmt and diff checks. The three original lookup assertions fail on the prior
implementation (12 driver compatibility controls GREEN), then pass unchanged
with single-resolution admission. Both independent correctness/adversarial
source/API/preservation reviews are CLEAR. Exact wrong-phase expiry denial,
invalid-query queue retention and no callback for unknown/retired/legacy socket
controls pass. No CPU improvement is claimed until the next exact CI measure.

### Phase F — stable shared driver storage during Cx leases

Actual SafeIDs #1789 is genuinely integrated at a31c3ff. Its entire tree is
identical to the reviewed 7beda tree, which already contained frozen SafeIDs b74;
the 183.426 -> 204.241ms Bitunix difference is a same-source runner observation,
not proof that adding the actual merge introduced allocator work. Actual landing
still passes Bitunix public CPU (before SafeIDs: TS201.1/Rust164.9ms; after:
TS197.0/Rust172.9ms). The socket a31 result is a real failure: TS198.193 /
Rust204.241ms versus178.3737ms target (+14.50%), with no retry. Its OKX inverse
CPU also misses, while EEA memory and Bitget public CPU pass. Fix the shared
socket path before a fresh CI measurement; do not change budgets or thresholds.

Read-only cause/API audits find no ordinary current public-frame ID reservation,
signing allocation or timer rearm. Both fresh admission checks remain required.
The concrete representation work is `with_socket_driver`: each ingress/control
lease takes and restores a complete inline Driver, constructing an empty
Driver/Lifecycle each time. This is empty-container construction and state
movement, not a current per-frame heap allocation. Savings remain a hypothesis
until CI measures them.

Introduce a crate-private `DriverSlot::{Inactive,Ready(Box<Driver>),Leased}`.
Its explicit inactive and leased states cannot be confused. An inactive engine has no heap driver. First successful
feed registration validates/registers one local Driver, then boxes it once;
failed registration leaves the slot inactive. Later registration uses that same
owner. Inactive epoch/auth queries keep returning None/false, connect/proposal
validation keeps its existing typed invalid-feed refusal, and stop/retire remain
inert when no driver exists. No generic inactive query constructs a Driver.

An active Cx lease moves only the Box pointer, marks the slot leased, invokes the
same synchronous Driver operation and restores the exact pointer before native
codecs/effect callbacks. Missing/reentrant active access fails closed; it never
constructs a default registry or treats a borrowed managed registry as legacy.
Keep this safe Rust; no raw aliasing, unsafe pointers, cached epoch/admission or
new permission state. The Leased variant guards representation access only. Existing
SafeIDs fatal latches still prevent any output/callback after caught exhaustion.

Keep public APIs, Driver/Supervisor transitions, policy/time/phase/expiry order,
both fresh ingress/control checks, event-scope HTTP-delivery isolation, signing
barriers and fatal checks unchanged. A narrow holder may provide checked
read access plus Deref/DerefMut for already-active internal sites, but cannot
initialize itself through Deref or recover a missing leased driver. Registration
is the only initialization boundary. Do not broaden Supervisor timer/entropy
interfaces or touch allocator/HTTP/venue/harness/profile policy.

First commit assertion-RED controls observing registered Lifecycle pointee
identity before/inside/after the existing Cx lease for both native-control and
control-free declarations. Old inline take/restore moves it; the stable holder
must not. Add ordinary-message compatibility controls proving no new Io IDs or
timer/signing actions, native delivery and both fresh admission boundaries.
Retain all existing expiry/invalid-time/stale-generation/reconnect/auth/wire,
HTTP-delivery and caught-fatal controls. Review concrete holder access and panic
boundaries, then implement only shared storage/access adaptations. Run focused
engine/OKX/Bitunix/Binance native/strict/fmt/bans checks and two independent
ordering/preservation reviews. A new exact-head CI run must establish CPU and
memory objectives; storage stability is not a CPU-pass or margin claim.

Phase F disposition: the fixed same-runner control (Rust workflow
`39905975879`, diagnostic head `5be1d1a5794`) measured landing/pre-holder/holder
Rust medians of 92.004/103.5265/103.369 ms, retaining all 54 pairs per variant.
The holder closes 1.37% of the absolute gap; all six holder blocks remain slower
than landing. It does not establish a CPU fix. The holder and its pointer-only
invariants are reverted; the independent no-ID/no-watchdog-rearm, fresh-admission,
fatal and HTTP-isolation controls remain. Subsequent work follows sampled native
profiles rather than the storage hypothesis.

### Phase G — profile the shared idle socket-work boundary

The four native release samples compare exact landing `7641650e7b1` with the
pre-holder socket head `a31c3ff2a07` on one Mac in landing/socket/socket/landing
order. Each process replays the same 28,687 public frames for 40 seconds after
three warmups; macOS `sample` captures 20 seconds at 1 ms. Compiler, release
profiles, frame-pointer setting, dependencies, tape, driver and final native
facts match. Both independent profile readers validate the raw callgraph self
counts and the resulting function deltas.

Native CPU per tape averages 30.144 ms at landing and 35.113 ms with the socket
core. The new `process_socket_work` function accounts for 0.768/0.784 ms of
estimated self CPU per tape in the two socket samples; ingress and dispatch
account for about 0.696 and 0.441 ms. The apparent 1.074 ms increase under
`Message::from_str` is offset by 1.261 ms less under its `on_message` caller:
changed inlining is not evidence of an additional parse. Native whole-tape
setup/HTTP/drop stacks and sampled self estimates identify targets; they are
not measurements of WASM/Linux steady-state savings.

Concrete change: retain `process_socket_work` as a thin inline wrapper. Its
existing fatal check stays first, followed by the existing managed-socket
check and an exact `effects.is_empty() && proposals.is_empty()` return. Move
the original nonempty work loop unchanged into one private helper. Preserve
all call sites, each loop's fatal checks, capacity behavior, retirement order,
reentrant work and the existing HTTP-delivery/fresh-admission/final-drain
boundaries. Pending subscription frames and one-shot frames keep their current
release path; they were not processed by this loop. No cached admission,
additional allocator, venue workaround or gate/profile policy change.

Validation: retain the healthy no-ID/no-watchdog test and all existing socket
queue, effect, timer, expiry, fatal and HTTP-isolation controls; run the engine,
OKX, Bitunix and Binance tests and strict checks. Then repeat the same native
ABBA sampling against the saved landing binary and report the remaining gap
and next actual function delta before another change. If a large gap remains,
continue profiling before fresh CI. Final performance evidence still requires
the fixed same-runner CI-WASM comparison and fresh full CI, not native results
or favorable reruns.

Phase G first measured result (`9f992fdff2c`): the repeated native ABBA means
are 29.668 ms/tape at landing and 34.263 ms/tape at the candidate, leaving a
large 4.595 ms gap. `process_socket_work` still has about 0.622 ms of estimated
self time. The native symbol table contains that function but no separate
`process_pending_socket_work`: optimization folded the heavy loop back into the
wrapper. This is compiled evidence that the intended boundary was not retained,
not a claim of an additional protocol cost.

Refine only code-generation annotations: make the small wrapper inline-always
and the unchanged processing helper inline-never. Preserve the same checks,
loop bytes and all call sites. Rebuild with the same native driver/flags, verify
the compiled helper boundary, then repeat the same native ABBA sampling and
report the remaining gap before fresh CI or further source changes.

Phase G annotation result (`a2d3b8ca389`): the compiled native binary retains
`process_pending_socket_work` and has no separate wrapper symbol. The full
fixed ABBA cohort has only two self samples in the heavy helper. This confirms
the intended code boundary, not zero inclusive work or a speedup. The host was
heavily contended: each process received only 17.21–22.87 seconds of total CPU
in about 40 seconds wall time, compared with approximately 38–39 CPU seconds
in the earlier cohorts. Observed mean CPU/tape is 33.747/38.943 ms at
landing/candidate, a remaining 5.196 ms gap. All four semantic fact hashes,
frame counts, send counts and bytes match. Preserve the complete cohort;
do not select samples or certify quantitative closure from it. A separately
named complete ABBA repeat records the subsequently quieter host period.
That separate repeat observes landing 30.192/30.793 ms and candidate
34.564/34.633 ms CPU/tape, with 34.80–37.88 CPU seconds per 40 wall seconds.
The residual is 4.106 ms; the gap remains substantial. All four native facts
match again. The prior contended cohort remains retained in full. These are
separate native observations, not selected WASM gate results or a causal
cross-cohort estimate. The quieter repeat still attributes approximately
0.497 ms to shared dispatch, 0.142 ms additional dispatch-head inspection,
and 0.136 ms additional action-queue destruction.

### Phase H — bypass the completely idle shared dispatch queue

The native profiles consistently attribute approximately 0.44–0.50 ms/tape
to the socket candidate's shared `dispatch_actions`, alongside `Dispatch::head`,
`next_work`, `VecDeque` and destruction work. The socket drain intentionally
checks dispatch three times around its existing frame/fence boundaries;
landing checks it once. Preserve those boundaries and all their call sites.

Add one read-only `Dispatch::is_empty()` query that checks both `pending` and
`outbound`. Wrap `dispatch_actions` with a fatal-first, inline-always check:
return only when `io.actions`, `io.signing_admission`, and both dispatch maps
are empty. Otherwise enter the entire existing processing body unchanged in
an inline-never private helper. This retains the code-generation boundary
that Phase G's compiled experiment required. A `next_work().is_none()` shortcut
is invalid: it would miss plain queued work, parked providers and materialized
outbound metadata. Resource ID reservations alone confer no work authority.

The query and guard do not allocate, renew deadlines, consume readiness, call a
provider, touch HTTP delivery, or change any receipt, route, FIFO or socket
gate. Run the original nonempty body whenever any state could require it.
Check liveness before the early return, including a latched allocator failure
with no queued output. No admission result is cached; each socket ingress
continues its independent current-epoch/expiry check.
The idle return retains any existing empty action-vector capacity instead of
taking and dropping it; fresh CI retained-memory objectives remain mandatory.

Coordinate the read-only dispatch query with the execution-plan owner before
source edits. Independently review the concrete guard and byte preservation of
the original body. Verify existing mixed signed/plain ordering, parked signer,
queued materialized outbound, callback retry, socket retirement, caught fatal,
HTTP delivery isolation and full managed HTTP tests, plus focused engine/OKX/
Bitunix/Binance tests and strict checks. Repeat native release ABBA with the
same driver, tape, build settings and landing binary; retain every cohort and
report remaining gap and next self-time delta before further fixes. A large
remaining gap still blocks fresh CI. Final performance proof remains the
same-runner CI-WASM probe followed by fresh full CI.

### Isolated native result and Phase I: empty effect publication

Owner-authorized diagnostic run `39914791492`, physical job `120285805496`,
uses one isolated Linux runner. Its two complete fixed ABBA cohorts build and
compare landing `7641650e7b1`, original socket `a31c3ff2a07`, and Phase H
`d8dcb8e6e62`. All eight 40-second processes receive approximately 40 seconds
CPU; 20-second perf captures retain 159,756 samples with zero lost samples.
Both independent readers authenticate the full artifact, all 129 internal
hashes, three source/binary/lock/driver/tape contracts and every sample count.

Observed native CPU/tape means are 28.714454/36.279946/34.193569 ms for
landing/original/current. The changes close 27.58% of that observed gap;
5.479115 ms remains. This is native whole-tape diagnosis including setup/drop,
not a WASM steady-state gate or a completed performance fix. Self estimates
scale 20-second sample fractions by process CPU per timed plus three warmup
tapes. Unknown instruction addresses and compiler outlines are grouped rather
than falsely aligned across builds. Parsing attribution under `from_str` and
`on_message` nearly cancels; it does not establish an additional parse.

The next shared avoidable candidate is publication of empty effect vectors:
about 0.434935 ms in `VecDeque<Effect>::spec_extend` and 0.213987 ms in
`Cx::socket_effects`. Add a thin inline-always wrapper that returns only when
the incoming vector is empty AND the existing effect queue is within
`MAX_EFFECTS`. Preserve the entire current publication/overflow body in an
inline-never helper. The queue-bound check matters: an overfull existing queue
must retain its original exhaustion, retirement and cleanup behavior even if
the new vector is empty. The owned empty vector still drops on return.

No ingress admission, epoch lookup, time/expiry check, control codec, supervisor
traffic update, timer, resource reservation or HTTP-delivery boundary changes.
Add an overfull-plus-empty control and run existing overflow/retirement/fatal/
HTTP-isolation controls plus engine/OKX/Bitunix/Binance native and strict checks.
Independently reverse-check the preserved helper body and all call sites.
Repeat the same isolated CI-native before/current/landing protocol with a new
exact current source pin; retain every earlier cohort. Report remaining gap
and actual profile delta before another fix or fresh full CI. Substantial
remaining cost still requires investigation; final proof remains same-runner
CI-WASM then full CI.

### Phase J: in-place ordinary message admission and observation

The owner supersedes the unimplemented Phase I micro-fix with a structural
message-path split. Independent callchain accounting of isolated run
39914791492 attributes the full 5.479115 ms/tape gap: disjoint inclusive event
handling +4.204221, drain/flush +1.151232, and outside those boundaries
+0.123662 ms. A nonoverlapping subtree partition assigns +1.116069 to event/
drain/HTTP shell, +0.939020 to venue codec/Store, +0.835603 to ingress,
+0.706329 to other lifecycle/replay, +0.705431 to effects, +0.644325 to remaining
harness callchains, +0.480404 to allocation/drop, +0.030034 to supervision and
+0.021900 to dispatch/fencing. These are sampled accounting estimates, not
recoverable savings: inlining shifts symbols and native setup/drop is included.
Both independent readers reproduce all 159,756 sample assignments. Raw
inclusive parent/child deltas are never added together.

Concrete changes are limited to engine_socket.rs, socket_driver.rs, socket.rs,
and their existing tests:

- Extract Driver::admit_ingress(now, socket, kind) from ingress's current
  admit_checked closure. It performs the same fresh socket lookup, epoch/phase
  resolution, runtime TimeBounds validation and Supervisor::expired check.
  Admission::Expired still revokes the epoch synchronously in Lifecycle.
- Extract finish_ingress(cx, admission, kind) containing the existing entire
  transition/effect match. Existing ingress delegates to both, preserving all
  send/open/close/timer users. Engine's Message/Binary ingress calls the
  admission in place; only Expired enters the original Cx cleanup path before
  any codec callback. Healthy/denied admission creates no effects vector and
  does not move Driver out of Io.
- Add a read-only Supervisor::idle_enabled query; no deadline arithmetic or
  scheduling changes. Add a compact crate-private ControlWork outcome and
  Driver::observe_control(now, epoch, &control). It independently calls the
  same fresh admit_ingress AFTER native classification. It then performs
  Lifecycle::traffic once, preserving pong_seen/refresh and the declared
  TextAfterFirstPong receive rule. Auth or refresh while idle is disabled
  returns cold work with the exact refresh/receive flags. Other traffic calls
  Supervisor::received(now) in place and completes without Cx/effects.
- Driver::finish_control consumes cold work immediately. Expired admission
  finishes existing runtime/signing/transport retirement before callbacks;
  first-pong activation calls set_idle_enabled(cx,true) BEFORE received(now),
  even if it emits no effects. Auth continues through the unchanged auth
  transition body, including retries, permanent sibling refusal and cleanup.
  The existing control wrapper delegates to observe/finish for its users.
- Engine retains the exact native classification, startup-ready query, health
  observation, process_socket_work calls and final current(epoch) check. It
  handles Complete control in place and sends only actual cold work through
  socket_mutate. Both old pre/post socket_mutate assert_live fences remain at
  the equivalent admission/observation boundaries. No cached admission or
  collapsed check is permitted.
- Skipping empty effects publication must preserve its over-capacity edge:
  healthy/denied in-place outcomes check the existing effects queue bound and
  route an already overfull queue through the ORIGINAL empty publication
  cleanup. No global empty-effects micro-fix or queue/retirement reorder.

Do not modify handle_event_scoped, begin/count/finish_event_scope, dispatch,
drain, managed HTTP delivery, resource allocation, hosts, venue codecs,
subscription semantics or timer ownership. This is not the disproven holder
experiment: ordinary data no longer enters full-context transition/effect
machinery; first-pong/expiry/auth and all scheduling remain cold operations.
Empty Vec construction itself is not a heap allocation.

Plan review precedes source. Preserve and run the existing two-admissions /
128-lookup/no-new-ID/no-watchdog-renewal controls, exact stale/invalid clock and
delayed-expiry rejection, first-pong activation/timer actions, auth refusal,
retirement/invalidation, swallowed fatal, and HTTP-delivery isolation controls.
Add focused semantic edge controls if missing, including overfull effects with
ordinary in-place traffic. Run engine/OKX/Bitunix/Binance all-target tests,
strict all-target/all-feature Clippy, formatting and all ratchets. Independent
source/order reviews reverse-check cold bodies and all event/drain fences.

Then update only the authorized temporary diagnostic branch's source contract
for a NEW isolated fixed ABBA experiment, retaining every prior cohort. Compare
landing/original/current per-tape CPU, callchains and native facts on the same
runner. Target most of the remaining 5.48 ms; no savings claim comes from code
inspection. Significant remaining cost requires further profile investigation.
Same-runner WASM measurement and fresh full CI follow sufficient improvement;
no threshold/profile change or gate waiver is part of this work.

Phase J source verification: both independent plan and source/order reviews
are CLEAR. The original ingress transition match and complete auth transition
body remain byte-identical. engine.rs, http_lifecycle.rs, socket_lifecycle.rs
and signing_dispatch.rs are unchanged. Both cold helpers are inline-never to
keep transition machinery outside ordinary observation. Two semantic capacity
controls cover the initial empty-publication boundary and the second denied
control boundary while retired effects remain overfull. The initial test seeded
work before the common prelude and correctly failed its Ping assertion; the
corrected fixture directly targets managed ingress without changing production
ordering. All 17 focused socket tests and 1,458 four-crate all-target/all-feature
native tests pass, with strict Clippy, formatting, bans and ratchets. These
checks establish behavior preservation, not a CPU improvement. A new isolated
measurement will compare immediate pre-change d8dcb8e against this source and
the same fixed landing7641650 before any performance claim.

### Phase K: bounded active index and transactional coverage scratch

The owner timeboxes further socket work to AT MOST TWO measured iterations
following J. This is proposed iteration 1/2; iteration 2 must be justified by
its resulting profile. J's fresh paired gaps are 4.789487 ms (H) and 3.680175 ms
(J), only 23.16% closed. Both independent DATA readers authenticate all eight
processes and 159,787 samples. The unchanged exact gate is Rust <=0.90*TS, not
a literal 1.10 ratio; landing ratio1.14 tolerates only 2.60% extra Rust CPU.
Native whole-tape and WASM steady CPU remain distinct measurements.

The next measured admission target is Lifecycle::admit_checked (~0.568 ms self).
Its Linux disassembly is retained at
/tmp/codex-socket-phaseJ-admit-disassembly.txt. LLVM already removes the apparent
second epoch comparison after the fresh supervision closure; do NOT propose
that source-only cleanup as a CPU fix. The BTreeMap socket lookup and its
pointer traversal remain in the hot instructions. Setup/replay coverage builds
additional BTreeSets and Arc-key clones whose drop/insert samples are visible.

Changes are shared storage/algorithm substitutions, with no admission cache:

- In socket_lifecycle.rs, replace the bounded active socket BTreeMap<u64,Epoch>
  with a sorted Vec<Epoch>, ordered by epoch.socket.0. Each query still performs
  a fresh binary_search_by_key, then the exact current(feed) predicate. No scan
  of all runtimes, direct-ID array, inferred feed, reused ID or retained permit.
  The registry remains bounded by MAX_FEEDS=512 and stores only active epochs.
- begin checks zero/duplicate IDs by binary search before generation/state
  mutation, retains every can_begin/generation guard, then inserts at the sorted
  index at the existing commit point. It supports nonmonotonic valid IDs; no
  allocator-order assumption. retire removes the exact owned index entry and
  preserves its debug assertion, invalidation, phase, interests and cleanup.
  Existing socket_lookup_count remains at the actual fresh lookup boundary.
- In replace_with_repair, both removed and added vectors already follow unique
  key order from their source BTreeMap values. Replace their temporary expected
  and covered BTreeSets with items.binary_search_by(key), a Vec<bool> of exact
  items.len(), and a covered count. Reject missing and repeated keys (within or
  across batches), latch failures exactly as before and require count==len.
  All frame/byte budgets, native encoder calls and ordering, batch ownership,
  revision checks and no-fallible-work commit point stay unchanged. There is
  no change to desired/released maps, wire frames or retained memory custody.
- Gate the new private Supervisor::idle_enabled getter with managed-sockets:
  its only consumer is the gated driver. Verify feature-off strict compilation
  as well as all-feature gates. No clock or timer behavior changes.

Plan and independent audit precede source. Add semantic controls for arbitrary
/reversed and >u32 socket IDs, duplicate refusal, exact retirement/reconnect
lookup, plus unknown/duplicate/incomplete/ignored-error encoder coverage. Reuse
all existing transaction rollback, over-budget, many-retired-ID, two-fresh-query,
first-pong/auth/expiry, fatal and HTTP-isolation controls. Run four-crate native
all-target/all-feature tests, strict Clippy/fmt/bans and feature-off engine
Clippy. Independently review whole cold transition/coverage transaction paths.

Then ONE fresh isolated fixed ABBA measures frozen J as before, K as current,
and the same landing7641650, with all source/tree/driver/lock/facts pinned and
all samples retained. Report paired controls, remaining cost and exact gate
math. No build-policy/threshold change, open-ended iteration or WASM/full CI
claim. The temporary measurement branch never merges. After at most one more
measured iteration following this, report feasibility/options to the owner.

Phase K source verification: both independent plan and full source/transaction
reviews are CLEAR. New controls cover full-width nonmonotonic socket IDs,
shifted neighbors after retirement/reconnect, reverse native coverage order,
the same keys in both unsubscribe and subscribe, and sticky unknown/cross-batch
duplicate errors after valid output with complete rollback. All 1,461 native
four-crate all-target/all-feature tests pass, as do strict Clippy in both engine
feature modes, formatting, bans and shrink-only ratchets. The original
feature-off unused-getter failure is retained as a RED control. HTTP, dispatch,
driver, engine ingress and supervision behavior remain unchanged. These checks
establish preservation only; the fresh isolated measurement is still required
before any CPU improvement or gate-feasibility conclusion.

### Phase K isolated result and final iteration L

Both independent DATA readers authenticate run39925752443/job120321583930,
artifact12489841767 SHA256
f056ca2150188e6adb06bcae3b576d00c62e22e2c955b5bd322eb4cf6d18dfc6.
All136 ZIP members/129 internal hashes/159761 samples/eight records and three
source/build/binary identities pass. Own-cohort J is31.4295471ms landing versus
35.9597417ms candidate (4.5301947ms excess); K is31.4345662 versus36.2056813
(4.7711150ms excess). Observed excess increased0.2409204ms; no savings or causal
slowdown conclusion follows. Initial derived self-profile metadata incorrectly
retained J's artifact ID and an H-before label. Both labels are corrected, the
initial derived files retained and the unchanged complete numerical maps checked
independently. Raw samples and every historical experiment remain untouched.

The owner allows ONE final measured iteration, then a gate-math/options report.
The largest disjoint bucket is native codec/Store +1.71347ms; net native
on_message is +1.69156ms. The separate parser's +17.4ms inclusive row represents
changed inlining attribution, not an extra public parse. Public ControlPolicy
None already skips native classification. ELF inspection shows landing has one
static native text callback callsite; J/K have two (managed and legacy).
Both use the same parser also referenced by the private classifier. K/J native
on_message is16746B plus a2883B parser, versus landing21571B with parser inlined.
These facts motivate a codegen experiment; they do not prove its cause or savings.

First remove K's unhelpful production storage substitutions in a NEW commit,
restoring the entire lifecycle production prefix byte-for-byte from verified J.
Keep K's meaningful full-width/nonmonotonic/retirement and transactional coverage
controls, adapting only their private representation assertions back to the
BTreeMap. Keep the actual feature-off getter fix. Do not rewrite any prior commit
or erase the failed measurement. The last comparison uses verified J directly as
before, so no unproven K storage optimization remains in the shipping candidate.

The final shared change is one private #[inline(never)] Engine delivery helper
whose complete body is the existing venue.on_message(cx,socket,text) call. Both
the managed data path in engine_socket.rs and legacy text arm in engine.rs call
that helper using their ORIGINAL venue/Cx/socket/text arguments. All existing Cx
construction stays in place, so the helper takes separate &mut V and &mut Cx
rather than borrowing/reconstructing the entire Engine. It may compile to a tail
call, but that must be inspected rather than assumed. Gate the helper and legacy
call substitution on managed-sockets; feature-off builds retain the original
direct native call. No new Venue API, identity, routing lookup, Context, retained
marker, driver cache, header, parser or per-venue codec change is permitted.

Keep all admission/current/expiry checks at the exact original boundaries,
including two independent fresh message checks and the final current epoch
predicate. Keep health, startup-ready/classification ordering, capacity cleanup,
HTTP delivery isolation, event custody, fatal checks, dispatch/fence/frame/timer
and drain rails byte-identical. No forward-data enum or delayed callback path is
needed; both callsites remain at their existing positions. The change targets
the largest net profile delta, but the existing native callback is already
out-of-line and the parser still has two callers: the helper may simply add
call overhead. Do not label a source-only codegen change an improvement.

Plan and two independent API/order audits precede source. Run engine/OKX/
Bitunix/Binance all-target/all-feature native tests, strict Clippy in both feature
modes, formatting and all bans/ratchets; review the complete diff and unchanged
callback/context/HTTP order. Then freeze and run ONE final isolated identical
8-process ABBA (fixed landing7641650, beforeJ1c243, currentL), retaining all prior
cohorts. Inspect emitted callsites/code sizes as well as every sample and profile.
Fresh capture, complete independent DATA audits and honest own-cohort arithmetic
are mandatory. No extra experiment or full-CI luck follows this timebox.

Report exact gate math Rust<=0.90*TS (landing ratio1.14 affords2.60% extra Rust
CPU), native-versus-WASM limits and concrete choices after this measurement.
Even hypothetical recovery of the full1.7ms codec bucket leaves about3.06ms of
the observed native excess. Choices for the owner/Sam include retaining the
existing temporary managed-sockets boundary while deferring Bitunix adoption,
a deeper redesign, or an explicitly decided Bitunix-specific objective. Do not
change any threshold, profile or adoption gate without that decision. Same-runner
WASM and fresh final CI/proof remain required before a performance-ready claim.

Phase L preservation verification is complete: both independent plan and source
reviews are CLEAR. Removing only the helper and its two substitutions recovers
engine.rs and engine_socket.rs byte-for-byte from the rollback checkpoint. The
lifecycle production prefix is exact verified J; all 46 lifecycle test names
remain, with only two private representation assertions adapted. All 1,461
four-crate native tests across 70 targets, strict all-feature and feature-off
Clippy, formatting, bans and shrink-only ratchets pass. No callback guard,
Context construction, health update, expiry/HTTP/fatal/event/drain boundary or
native codec moved. This establishes preservation only. The last isolated
measurement and emitted-code inspection decide whether the helper is useful.

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

## Committable implementation sequence (not started)

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
is concrete API review and native original-code controls; no production edits
are authorized by this plan checkpoint alone.

# Backpack public and private socket lifecycle

Status: plan-only on actual landing tip34999f973f9ac9581cd4342d5cf23e4fa06c0142,
which includes account-read #2139 at3f2c90991c26cbf639a5001095bea962c9e1e295.
Owner codex-mxb4mq; native/API reviewer codex-hcmq3f. No runtime, tests, Cargo
feature, inventories or CI changes at this checkpoint. Overseer has approved BOTH native lifecycles to resolve the mixed-mode blocker.
Source/originals remain HELD until hcmq3f reviews this full-scope API plan.

## Approved slice and inventory

Two native socket connection sites, each count1:

- venues/backpack/src/lib.rs public:
  4e80f004df9eb74b86dea4ec557618f2601e8b378fcdfefb0dd1c4d8f0b985ab.
- venues/backpack/src/account.rs private:
  a11771673f092a01c7fac3d204d1c1f3dd8424214a324efce5390248d48ecfbe.

Expected venue source delta400–600 lines plus controls. Adopt existing managed
feed/epoch/interest lifecycle for BOTH sockets atomically. Both connection sites
leave the socket inventory in this PR. Shared Connection can be replaced only
after both users move; preserve native financial/cache/publication state.

Overseer full-scope ruling excludes ALL signer, credential and generic engine
migration. Private auth wire and signing MUST stay byte-identical, proven by
existing wire/replay controls. Account-read/repair, HTTP/history/book reducers
and credential selection remain unchanged. The separately reviewed shared
repair plan/core is not a prerequisite and confers no repair source authority.
No generic coexistence seam or unknown-socket exemption will be introduced.

## Scope disposition and current API fit

At the pinned tip Backpack does not enable engine managed-sockets. OKX/Phemex
already do. Enabling the feature alone is not the coexistence problem: the first
register_socket_feed sets the engine-wide core.managed_sockets=true.

- engine_socket.rs managed_send_allowed requires every outgoing socket to have
  a registered epoch once that flag is set.
- socket_gate applies lifecycle admission to both connect/send and refuses an
  unregistered socket; fence_socket_actions removes its pending connect/send.
- handle_managed_socket consumes unregistered ingress rather than dispatching
  legacy private handlers after managed mode is active.
- socket_lifecycle.rs legacy_and_unknown_managed_ingress_are_distinct supplies
  a working state boundary: unknown ID is Legacy with no feed, Denied after a
  feed is registered, and Current only after a real epoch is begun.

Backpack account.rs still uses an unregistered private cx.connect plus raw
native subscription authentication, and super::Connection. A public-only feed
registration therefore cannot honestly promise that private connect, auth,
ingress and reconnect remain functional. The incoming34999 change only alters
interest-memory ownership in socket_lifecycle.rs; it adds no coexistence seam.
These are source/API observations, not a reproduced candidate runtime failure.

Overseer ruling resolves the scope blocker by adopting BOTH native lifecycles,
not a generic coexistence seam. hcmq3f must review the exact mapping below before
originals. No inert private epoch, unknown-ID exemption or core gate weakening.

Proposed registration is once for both real FeedIds in ordinary engine startup
before either physical connect can be queued. Never register inside a fresh
user-command callback ahead of its admission. Policy registration is not a
credential grant: native account_start still determines whether private connect
is eligible; public-only never starts private work, including saved read keys.
Each real connection uses connect_socket and its returned epoch, with native
WS URL/SocketChannel/routing unchanged. Forwarded/suppressed public feed stays
NotPermitted with no fake connection or reconnect loop.

## Native public behavior to preserve

Public startup follows accepted catalogue/tickers/markPrices seed completion.
There is no public auth. Preserve the native supervision policy:20s idle,
5s ping,1s base/16s capped backoff and existing Config defaults. Every current
text observation refreshes idle;4002 is the native latency/pong response. Keep
STREAM_MALFORMED diagnostics, other error handling and half-roundtrip latency.

Preserve topic wire and ordering for ticker/trade/bookTicker per market, depth
per active book, and kline per candle interest. Subscription/unsubscription
methods, JSON params, native interest ordering and suppression semantics remain
exact. A reconnect invalidates every book generation/buffer bridge before its
new snapshot; current subscription handoff precedes that HTTP observation.
Real managed interest release/after-interest continuations own this ordering;
queuing a frame is not proof of handoff. Keep existing public ticker overlay,
book buffering/bridge/recovery and candle payload mappers byte-preserved.

Preserve PublicFeedPolicy::Direct versus suppressed feeds. A suppressed public
transport never creates a connect/backoff loop; do not fake an epoch or treat
NotPermitted as a transient network error. Any new shared resource-capacity or
nonfinite behavior must be named as such in CALLERS, without changing native
healthy policy or inventing new retry authority.

The source proposal maps both native open/message/close/timer effects to real
engine LifecycleEvent/Control and transactional public interests. Remove both
venue supervisors only once their shared-engine effects preserve each native
policy; there is no second backoff/idle clock in venue state. Keep only codec,
interests, epoch/feed references and native financial/publication state.

## Native private behavior and API-review pins

Private connect still begins only from native-ready account_start and its
existing reconnect continuation. The same Connection defaults provide20s idle,
5s ping and1s–16s backoff. Do not invent an authentication timeout or introduce a
new ready/auth acknowledgement that native does not await.

Native private_open performs, in this exact order: compute c.ws_auth(cx.now),
send SUBSCRIBE for account.orderUpdate and account.positionUpdate, set the native
open/ping state and supervisor-open equivalent, send PING, then (only if Orders
already loaded) issue Orders(None), Positions(None), Balance(false,None). The
new managed epoch must permit the exact signed frame before these follow-ups.
Preserve the serialized SUBSCRIBE fields/params/signature and c.ws_auth body
byte-for-byte: wire_stamp_ms, payload subscribe, empty Params, Ed25519, base64,
primary key and fixed5000 auth window. This is neither HTTP receive-window
selection nor SelectedRead signing. sign.rs is out of source scope.

Private native message handling refreshes idle on any text. Error4002 returns
without latency publication; other errors close/retire the socket and retain
STREAM_UNAVAILABLE text. Keep every order/position/fill reducer, order token
invalidation, stream timestamp,500ms fill throttle and10s poll scheduling exact.
Private close resets only socket/open and schedules native reconnect; it does
not acquire new account snapshot or repair Mutation authority.

API review must disposition AuthMode::None with Native controls/AnyText for the
private lifecycle: native signed subscription authenticates on wire but has no
separate native accepted-auth readiness gate. Do not call AuthMode::Required or
fabricate AuthEvidence::Accepted merely to satisfy managed readiness. Review
whether Opened versus Ready delivers the native private opening sequence and
how existing raw cx.send is gated in that exact managed epoch. The signed frame
is native one-shot authentication, not a persistent subscription interest that
can replay with an old signature. Current-epoch raw signing/wire is intentionally
preserved for this approved lifecycle-only slice, not claimed as managed signing.

Original native on_message/on_open/on_close handlers cannot remain as parallel
managed ingress paths. Shared engine admits current epoch once, then invokes
native codecs/effect handlers. Replayed and stale ingress is denied before all
native health, notice, Store and account-read effects.

## Originals and controls after plan/API scope CLEAR

Use real physical connect IDs, engine-issued epochs, Open/Message/Close/Timer
and actual frame drain; no forged lifecycle handles or simulated handoff.

| Case | Required observation |
| --- | --- |
| Healthy seed/open | Exact native public subscriptions, initial PING and public loaded state; private startup continues under the approved BOTH-lifecycle scope. |
| Before open/stale epoch | No ticker/book/candle Store mutation, diagnostic or read successor from unopened, retired or foreign ingress. |
| Reconnect | Old generation inert; exact native reconnect/backoff; subscribe release before each real new depth snapshot; buffered financial assertions retained. |
| Traffic/health | Any native current text refreshes idle as before; exact4002 pong latency and5s ping; idle equality20s, no copied deadline. |
| Interests | Add/remove/refcount/replay and capacity are transactional; no partial frames, duplicate topics, leaked interest credits or lost healthy peer subscriptions. |
| Policy | Direct/public-only healthy; suppressed feed no connect/retry; private credentials/auth/account chain remain exact in the approved scope. |
| Hostile time | NaN and both infinities issue no new connect/send/timer or financial publication; real cleanup retires exact resources. |
| Both feeds | Prove private connect/auth/message/reconnect remain functional before and after public startup, and independent public/private reconnect does not retire its sibling feed. |
| Private auth wire | Existing fixed signature/timestamp/base64/key and5000-window controls plus replay retain exact SUBSCRIBE/PING bytes/order; no secondary/login fallback, changed signing material or new auth gate. |
| Private read order | First open versus reconnect preserves loaded Orders guard and exact Orders/Positions/Balance issue order;10s poll and500ms fill behavior unchanged. |
| Scope separation | Account-read provider/signature, Context retirement, repair/native trigger tests remain exact; socket expiry cannot grant repair or new send authority. |
| Native payloads | Existing ticker/trade/bookTicker/depth/kline financial rows, precision, volume, bridge and recovery assertions stay unchanged. |

Keep native working controls separate from new-contract original REDs. Preserve
existing assertion bodies, payloads and event clocks; fixture adoption requires
an exact artifact/owner notice and independent preservation review. At source
stage execute all formerly gated suffixes and full credit recovery.

## Overlap and source scope gates

Open-PR audit before the pause found #2132 D1 on sign.rs/trading.rs and recipe
fixtures, #2033 on managed/support fixtures, and #1961 on trading/provenance.
#2139 is actually landed. Refresh these file lists/local branches before source;
no other public socket source owner was found at that checkpoint. Notify shared
fixture owners before any support helper edits. Merge ACTUAL prerequisites;
never assume a stack, rebase or force-push.

## Measurements, checks and landing

Overseer notes managed-sockets adds about47K to a newly linked family, within
Backpack's roughly250K headroom. Combined already links it through OKX/Phemex,
so expected incremental linkage is small. These are planning estimates, not
measurements or permission to raise budgets.

Overseer requires this paired A/B gate to be fixed in the plan BEFORE source:
measure bare ACTUAL landing tip versus composed first-source tree
for Backpack and combined with the same toolchain, optimizer and settings.
Merge the actual landing tip before every required composition measurement.
Generic engine source/coexistence work is explicitly excluded by the full-scope
ruling, so Bybit/Bitget builds are not part of this venue-only plan. Delete each OWN scratch target after its completed
measurement, retaining source/artifacts/logs/table; no other lane targets.

Plan/API and original controls CLEAR precede production. Then scoped Backpack
and affected registry tests, formatting, strict Clippy, bans/all shrinking maps (socket manifest removes both exact count1 entries; signing,
credential and request inventories stay incoming-exact),
exact unowned preservation, source/fixture review and paired size gates. One
frozen push, one PR-triggered qualifying run; no WIP pushes or duplicate dispatch.
On full green plus source CLEAR, mark READY and send head/tree/size/run evidence
to auditor8x5tm1. Auditor exact-tree GO precedes the overseer's merge. No source,
original execution, build/size, CI, readiness or landing proof is claimed here.

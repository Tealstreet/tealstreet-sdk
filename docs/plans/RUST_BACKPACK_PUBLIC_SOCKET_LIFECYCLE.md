# Backpack public socket lifecycle

Status: plan-only on actual landing tip34999f973f9ac9581cd4342d5cf23e4fa06c0142,
which includes account-read #2139 at3f2c90991c26cbf639a5001095bea962c9e1e295.
Owner codex-mxb4mq; native/API reviewer codex-hcmq3f. No runtime, tests, Cargo
feature, inventories or CI changes at this checkpoint. Source/originals HELD on
the mixed managed/legacy socket blocker below.

## Approved slice and inventory

One public socket connection site in venues/backpack/src/lib.rs:
4e80f004df9eb74b86dea4ec557618f2601e8b378fcdfefb0dd1c4d8f0b985ab count1.
Expected public-only venue source delta200–350 lines plus controls. Adopt the
existing managed feed/epoch/interest lifecycle, preserving native payload and
financial reducers. Public socket code shares Connection with the private
account socket; no accidental removal of private state or supervision.

The approved public scope excludes private authentication/signing, account read
custody, repair Mutation, HTTP/history/book reducers and generic engine source.
A required scope extension must be explicit before originals or production.
The separate shared repair plan/core is not a prerequisite for socket adoption.

## Current blocking API fit

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

Ask hcmq3f to confirm this exact boundary before controls. Then obtain an overseer
scope ruling: either migrate BOTH native public/private socket lifecycles using
the current engine API (while separately preserving the existing private auth
wire/signing behavior), or use a separately owned/reviewed shared coexistence
seam. Do not register an inert private feed as a bypass, exempt unknown sockets,
weaken the core gate, silently migrate signing or expand source scope here.
Private lifecycle inclusion would be two inventory sites and roughly400–600
venue lines plus controls, subject to native API review. Shared engine work
would require its own approved scope and Bybit/Bitget size pairs.

## Native behavior to preserve after disposition

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

The initial source proposal must map native open/message/close/timer effects to
real engine LifecycleEvent/Control and transactional interests. No second venue
supervisor alongside engine supervision; remove only proven orphaned public
state, retaining private dependencies until explicitly adopted. Credentials,
read/write Context authority and signing recipes stay outside this slice unless
an overseer ruling expands them.

## Originals and controls after plan/API scope CLEAR

Use real physical connect IDs, engine-issued epochs, Open/Message/Close/Timer
and actual frame drain; no forged lifecycle handles or simulated handoff.

| Case | Required observation |
| --- | --- |
| Healthy seed/open | Exact native public subscriptions, initial PING and public loaded state; private startup continues under the ruled coexistence scope. |
| Before open/stale epoch | No ticker/book/candle Store mutation, diagnostic or read successor from unopened, retired or foreign ingress. |
| Reconnect | Old generation inert; exact native reconnect/backoff; subscribe release before each real new depth snapshot; buffered financial assertions retained. |
| Traffic/health | Any native current text refreshes idle as before; exact4002 pong latency and5s ping; idle equality20s, no copied deadline. |
| Interests | Add/remove/refcount/replay and capacity are transactional; no partial frames, duplicate topics, leaked interest credits or lost healthy peer subscriptions. |
| Policy | Direct/public-only healthy; suppressed feed no connect/retry; private credentials/auth/account chain remain exact in the approved scope. |
| Hostile time | NaN and both infinities issue no new connect/send/timer or financial publication; real cleanup retires exact resources. |
| Mixed/private boundary | Prove private connect/auth/message/reconnect remain functional after public feed registration; source-only assumptions do not close this gate. |
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

At the FIRST source commit measure paired bare actual tip versus composed tree
for Backpack and combined with the same toolchain, optimizer and settings.
Merge the actual landing tip before every required composition measurement.
Bybit/Bitget pairs are additionally required only if approved generic engine
source becomes necessary. Delete each OWN scratch target after its completed
measurement, retaining source/artifacts/logs/table; no other lane targets.

Plan/API and original controls CLEAR precede production. Then scoped Backpack
and affected registry tests, formatting, strict Clippy, bans/all shrinking maps,
exact unowned preservation, source/fixture review and paired size gates. One
frozen push, one PR-triggered qualifying run; no WIP pushes or duplicate dispatch.
On full green plus source CLEAR, mark READY and send head/tree/size/run evidence
to auditor8x5tm1. Auditor exact-tree GO precedes the overseer's merge. No source,
original execution, build/size, CI, readiness or landing proof is claimed here.

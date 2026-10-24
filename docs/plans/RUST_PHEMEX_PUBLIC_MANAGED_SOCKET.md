# Phemex public socket on the shared managed core

## Scope and ownership

Base: actual landing `abc72534026dc46fff4a0dd79c9438c8d459c5e4`.
Branch: `feat/rust-phemex-public-managed-socket`.
Owner: codex-5c57jl. The request owner, codex-ih35js, confirms no active or
imminent Phemex lib.rs/public-socket claim; its candle/local-history PRs landed.

This is one public-feed slice, not a whole-family migration. It adopts the
landed managed-sockets core without engine, host or OKX source changes. Frozen
#1943 stays intact. Private account authentication, its snapshot deadline,
signing and native account supervisor remain legacy in this slice. Bitunix is
excluded under Sam's option 1 and remains a remove-before-master obligation.

Production scope: Phemex lib.rs public connection/interest/timer hooks, a new
native socket codec module, and its Cargo managed-sockets feature opt-in. Tests
use a new public-socket fixture file. HTTP selection/builders, public candle
Context/error hooks, history, income, identity, account, trading, credentials,
options and existing financial reducers remain unchanged. Shared manifests may
only prune the retired Phemex public socket fingerprint; no other entry changes.
No source file from #1943 is edited. Caller/quality evidence is recorded with
this plan while shared doc integration waits for the frozen train predecessor.

## Existing native behavior

The public URL is `wss://ws.phemex.com` or the existing testnet URL selected by
`ws()`, with the same Public channel and host route behavior. Public data comes
from the existing packed ticker, orderbook and kline decoders.

The public Supervisor declares connect 10s, idle 20s, ping 5s, initial retry
1s with cap30s and stable reset30s. Its receive path refreshes liveness before
JSON parsing, so the shared policy is AnyText with ControlPolicy::None and no
authentication. Preserve all these constants, including retry jitter.

At accepted open: fixed `perp_market24h_pack_p.subscribe`, retained book
subscriptions, retained candle subscriptions, then `server.ping`. Wire method,
params and incrementing native public RPC IDs remain venue codec facts. Book
subscribe uses `[nativeSymbol,false,0]`; unsubscribe uses `[nativeSymbol]`.
Kline subscribe/unsubscribe uses `[nativeSymbol,resolution]`.

Book HTTP seed starts on the first received depth frame, not on open or interest
admission. The existing generation, buffer, timeout, retry and sequence handling
stay native. This slice therefore needs no #1943 read-continuation API and adds
no local deferred-work queue. Private AOP snapshot and auth sequencing are out
of scope.

## Architecture

Replace only the public raw SocketId/open/Supervisor authority with one shared
FeedId/current epoch and native wire ID bookkeeping. Register the existing
Public policy once, connect through Cx::connect_socket, and use shared lifecycle
Ready/Retired/ping callbacks. Existing generic callbacks remain for private
legacy dispatch. Managed public ingress must not enter the private path twice.

Desired interests consist of the fixed ticker topic, refcounted active books
and refcounted active candles. Keep the existing 19 dynamic-interest limit and
native symbol/resolution validation. The codec projects desired canonical
interests and encodes shared subscribe/unsubscribe deltas using unchanged native
wire shapes. Do not add another timer owner, retry loop, physical-ID counter or
subscription queue. A native RPC request counter is only serialized wire data.

Use the landed send_after_socket_interests for the immediate opening ping so
all required subscription frames precede it. Periodic pings remain native
server.ping frames through the existing shared ping hook. Required retirement
invalidation preserves the current book-generation/buffer/deadline/retry cleanup
before a replacement epoch can publish. Duplicate open and late old ingress
must not replay subscriptions, refresh health or publish market data.

Suppressed public-feed mode must create no direct socket or retry churn. Reuse
the existing policy/host behavior; do not treat NotPermitted as a reason to
retry or create a second public path. No HTTP authority, completion or signer
semantics change.

## Plan, original REDs and verification

Commit this plan first, then assertion-only tests on the unchanged landing:

1. Duplicate WsOpened on the current open socket emits no second subscriptions
   or ping and does not renew the watchdog.
2. An unopened socket delivered WsOpened after its 10s connect deadline emits
   no subscription/ping, and enters normal shared retirement/retry.
3. A ticker frame after the 20s idle deadline, without delivering the watchdog
   timer first, cannot change the Store or revive the expired socket.
4. Working controls prove current open sends exact native ticker subscription
   before opening ping, live packed ticker updates retain their values, and
   stale retired ingress stays inert. These distinguish real REDs from setup
   or decoder failures. Keep the original assertion records after migration.

After owner/plan review, implement only the native public adapter. Require the
original REDs to turn green, refcount/diff/reconnect/suppressed-feed controls,
all existing Phemex public/account supervision fixtures, strict Clippy/fmt and
both unchanged ratchets. No weakening existing financial or wire assertions.
Fresh rebuilt CI must pass the unchanged Phemex public CPU, retained-memory,
size and replay objectives, plus combined engine feature modes. No profile,
threshold, reference or gate edits; no performance claim from native unit tests.

Inventory effect: remove exactly the current public lib.rs socket fingerprint
`bfc950bbe21c9d625ea9774177bfa8606bb559d376d71b7ad05a25603c20683a`
only if it matches the actual scanner output at implementation. The private
account.rs fingerprint remains. Record the exact map/count and preservation
proof in the PR body; never rehash an unrelated site to force a ratchet pass.
One venue per PR, fresh independent reviews and train GO before any merge.

## Original-source result

On exact `abc72534026` production, the dedicated real-engine target compiles
and runs all six controls: three expected assertion REDs and three working
GREENs, with zero ignored tests. The expired ticker changes last from 20690.2
to 21000; duplicate/late opens release native frames that the assertions prohibit.
The positive controls prove exact opening wire order, normal live packed-ticker
values and retired-socket inertness. This is a correctness checkpoint only,
not a baseline performance or full private-suite claim.

## Whole-venue ruling and parked checkpoint

The public-only implementation passes all six original controls, but the wider
Phemex account target fails 24/40 tests with account.rs/private production exact.
Once any managed feed is active, shared ingress intentionally does not retain
a public-managed/private-legacy dual path. Unregistered private ingress never
reaches native account callbacks. No fixture or financial assertion is weakened.

Overseer ruling: socket slices migrate a venue whole. Preserve this public WIP
and original controls for a later full Phemex public/private adoption; do not
publish this partial variant or add mixed-mode compatibility in core. A whole
Phemex plan must first include its connect-relative auth/snapshot deadline,
accepted financial snapshot proof and private ping/retry behavior. That scope
is not cleared by the preceding public-only policy review.

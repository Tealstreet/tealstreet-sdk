# Backpack public and private socket lifecycle

Status: source/API/native/fixture preservation CLEAR; scoped qualification and
local combined measurement pass on the D1 composition. Frozen-head CI is the
record for family sizes; green CI and auditor GO remain gates.
Original baseline is actual landing tip34999f973f9ac9581cd4342d5cf23e4fa06c0142,
which includes account-read #2139 at3f2c90991c26cbf639a5001095bea962c9e1e295.
Owner codex-mxb4mq; native/API reviewer codex-hcmq3f. Runtime, Cargo features, inventories and CI remain unchanged at the original
control checkpoint. Overseer approved BOTH native lifecycles. hcmq3f cleared
plan/API ccfdd72, originals3d541 and exact runtime c5e5; source is applied after
that CLEAR, with qualification evidence and remaining fixture gates below.

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
to auditor8x5tm1. Auditor exact-tree GO precedes the owner's guarded head-matched merge. No source,
original execution, build/size, CI, readiness or landing proof is claimed here.

## Original-control checkpoint

Plan/API direction CLEAR from hcmq3f at exactccfdd72. New isolated
socket_lifecycle target drives real installed Ed25519 provider setup and genuine
startup HTTP Prepare/Bind/Claim before capturing public/private socket IDs.
Production, Cargo features, inventories and existing fixture files are unchanged.

Executed on unchanged runtime: own19 controls =5 native working GREEN and14
intended assertion RED,0ignored. The existing account35 and public_supervision11
both pass unchanged. New REDs are financial publication before open/at idle
expiry/at NaN and both infinities, duplicate auth/subscription effects and native
diagnostics before open. No missing import/API, setup or compilation failure.
Private wire positives independently sign the literal native subscribe payload
with deterministic Ed25519 material and check the exact serialized frame, PING,
and Orders/Positions/Balance sequence, including no-ACK reconnect and public
suppression/private-positive mode. Source-stage timer/no-work assertions after
an earlier failing denial remain gated until source goes GREEN; no suffix proof
is inferred from the original RED receipt.

Original receipt/logs stay outside Git. Original/native-control review precedes
source application. Keep the19 bodies unchanged, add further lifecycle/pressure
controls at source stage, retain all native assertion/wire clocks, and execute
every gated suffix before sourcefreeze. No source, size/CI or READY claim here.

## Additive source-stage controls on unchanged runtime

The original19 target remains an entire byte-exact prefix. Eight additive
controls pin full Store wire snapshots and complete action capture at hostile
clocks/retired epochs, ordered auth/PING/physical-read trace, sibling survival,
public4002 half-RTT versus private no-latency, malformed AnyText liveness, the
unloaded-Orders opening guard and public-only startup with installed identities.
Unchanged-runtime target27:12 working GREEN,15 assertion RED,0ignored. One new
public-only expectation was corrected before this receipt: native public-only
marks account sections loaded while keeping orders/positions/balance empty.
That assertion correction is not an original regression claim. The original19
bodies and their5GREEN/14RED evidence remain unchanged.

The first source proposal e9cd3b77 is HELD for two hcm P2s: aggregate book
continuation cancellation after one unsubscribe, and nontransactional interest
refusal/whole-feed stop. Corrected source must use independently required depth
keys and commit/rollback native demand with actual interest transactions.
Additive real two-book wake and capacity/refusal/recovery controls remain gates.
No runtime, size, CI or READY claim accompanies these additive controls.

The three P2 regression originals also compile/run on unchanged runtime: target30
finishes12GREEN/18intendedassertRED/0ignored. They cover two independent book
wakes with removal before handoff (first-open and reconnect), real512 pending
handoff-credit refusal/release without phantom refcounts, and real proposal-byte
versus encoded-frame capacity refusal. Their deeper source-only suffixes remain
gated on legacy; no capacity/recovery execution proof is inferred from the RED.
Corrected immutable runtime7d43d006 remains UNAPPLIED pending hcm review.

The recovery preview42102c4f is UNAPPLIED. It gates all automatic paths on the
exact current public epoch, finite time, no pending cooldown and no exhaustion.
Only a real finite timer delivery spends one of three1s recovery observations;
commits do not reset the budget. Resolved demand cancels its exact timer, and
retirement clears it. Refused new additions/removals retain transactional rollback;
per-book genuine handoffs remain independent. Native book-recovery timer work
also requests that real handoff before the unchanged snapshot primitive.

Control corrections follow hcm's API finding: surviving desired interests receive
new wake IDs, so controls capture current wake/cleanup traces and keep every old
wake inert. A65536-byte symbol tests InvalidInterest, not encoder capacity. A
separate bounded-key batch grows past65536 encoded bytes, preserving earlier peer
release and proving recovery after that release. Persistent real513-book pressure,
same-clock commits, three-observation cap, all nonfinite timer deliveries and
obsolete-timer cancellation are separate controls. Engine Ready's own replay Err
remains inherited fail-closed; only venue proposal refusals get rollback/recovery.
No generic engine change or blanket replay recovery is promised.

On unchanged runtime target35 compiles and executes12workingGREEN/23assertionRED,
0ignored. One source-stage Notice enum access was fixed before this receipt; that
compile failure is excluded from original evidence. The original19 entire prefix
and the eight earlier source-stage controls are retained. Exact source/control
review, GREEN suffix execution, fixture preservation and paired sizes remain gates.

The next unapplied preview adds the two narrow42102 corrections: retry custody
retains its advertised not-before beside the genuine TimerId. Early delivery
rearms a newly issued timer to that SAME not-before without quota/proposal work;
consumed IDs are inert, equality spends one observation, and late delivery spends
one observation before scheduling from its actual delivery time. Nonfinite and
stale handling remains allocation-free. This is the bounded interest-observation
cadence, not another socket supervisor or HTTP/auth permission.

The native encoder now preflights MAX_FRAME_BYTES and returns Capacity before
emit for oversize. Exact65536 and one-extra-byte controls use valid bounded keys;
healthy private auth/wire and every other key/coverage validation stay unchanged.
Inherited Ready replay therefore also reports Capacity consistently.

Unchanged-runtime target37 compiles/runs12workingGREEN/25intendedassertRED,
0ignored. The original19 remain byte-exact. New early/duplicate/equality/late
and exact-fit/one-over suffixes remain gated until source execution. hcm has
separately CLEARed pure fixture-forwarding8f89dd21. Source remains UNAPPLIED
until exact cadence/frame delta CLEAR; paired size and qualification gates stay.

The exact c5e5 cadence/frame runtime and forwarding8f89 are now applied after
hcm SOURCE/API CLEAR. Lifecycle37/37 passes, including every former gated
capacity, nonfinite, cleanup and early/equality/late suffix. Merged native
baseline account35/supervision11 passed before application; initial qualification
then exposed fixture readiness assumptions, not compile or infrastructure errors.

The native fixture adaptation is narrow: five previously unopened private-data
tests get a real first Opened at their existing1000 clock and drain the resulting
real preparation/signing work. Supervision explicitly drives only its captured
current zero-delay snapshot handoff after the actual subscription release.
Only two repeated-reread fixtures supersede their observation clock/socket:
one_malformed_row_keeps_its_stored_copy_while_valid_peers_commit and
unproven_trigger_quantity_keeps_its_identity_while_valid_peers_commit_without_repair.
Their duplicate Opened events become genuine Close, captured retry at its
advertised time, new physical socket, one Opened, and same-time prepared read
responses. Payload financial timestamps and every existing assertion stay exact.
Unrelated ready/ws/reply helpers, fixtures and clocks remain unchanged. hcm's
fixture-direction ruling permits this necessary clock adaptation; exact artifact
preservation review and successful native rerun remain gates.

The additional account-read overlap sites native_live_stale_positions_keep_next_without_publication,
balance_overlap (including its254-peer fanout consumer), and
native_balance_older_than_committed_never_wins_after_two_more_discards now use
real first-open/settled refreshes before the original poll, then genuine reconnect
for the newer snapshot. Relative fill/discard/acceptance spacing follows that
actual opening. fill() derives wire E from the observation clock, so those generated
E values and observation-derived Store timestamps shift with it. This necessary
fixture-clock supersession is approved; raw financial literals, prices, amounts,
fill counts, all assertion predicates and production reducers remain unchanged.

Two further fixture-only clock supersessions are approved: public
rest_query_success_failure_and_stream_resubscription_have_independent_state
resubscribes and answers its held query/new seed at3 after its existing removal
at3; unrelated subscribe() callers retain0. Trading
reconciliation_created_at_cannot_invalidate_pending_cancel_acknowledgements
replaces duplicate Opened2100 with an actual Close/retry-at/new socket/Open and
prepared read at that observation time. ORDER.createdAt1000, last_update1000,
all financial assertions and the original cancellation request/ACK are unchanged.

## Committed source and first-source paired measurements

Source commit c47a5f83e7d, composed by plain merge at4df579711ee on actual
0a8c9625c497. hcm committed source/native/API/fixture/map carry is CLEAR.
All18 own paths and5980 incoming-unowned paths were independently verified;
only CALLERS resolved manually as entire incoming plus exact own suffix once.
Both socket count1 sites and the Backpack socket-family row are removed; all
foreign inventory entries/order and request/signing/credential maps stay exact.
All534 incoming assertions and original19 entire prefix remain intact.

On that actual composition:377 native PASS,0FAIL,1pre-existing ignored across
27targets; strict all-target Backpack Clippy, selected-file formatting and full
bans/ratchets PASS. The cleared forwarding formatter also inserts an optional
FnMut trailing comma; it is behavior-neutral, not literally token-identical.

Four sequential fresh-target builds used Rust1.99.0, wasm-bindgen0.2.129,
wasm-opt132, CARGO_BUILD_JOBS=4, BINARYEN_CORES=4 and
SAFE_CEX_WASM_OPT=required. Commands were the unchanged harness/build.sh
(combined) and harness/build.sh --venue backpack. Each tree's target was deleted
immediately after its measurement; sources/logs/artifacts remain outside Git.
Web and Node each produced the same bytes and SHA256 for each row.

| Bundle | Bare0a8c bytes | Composed4df bytes | Delta | Budget | Composed headroom |
| --- | ---: | ---: | ---: | ---: | ---: |
| Backpack | 1,092,442 | 1,159,729 | +67,287 | 1,343,488 | 183,759 |
| Combined | 14,034,260 | 14,053,513 | +19,253 | 14,155,776 | 102,263 |

Receipt: /tmp/codex-backpack-socket-measurement.json, with exact command/env,
source/tree pins, tool versions, web/Node artifact SHA256s and cleanup proof.
Native/strict/bans receipt: /tmp/codex-backpack-socket-composed-qualification.json.
No generic engine source is changed, so this slice requires Backpack/combined,
not additional Bybit/Bitget measurements. Frozen-head CI and auditor GO remain.

## Post-S1 and D1 composition evidence

Intermediate c66ed62c945 on actual3d30b88713d passed377 native tests,
0failed,1existing ignored; strict Clippy, owned formatting and full bans/ratchets
passed. Its completed local pair is retained only for those pins:

| Bundle | Bare bytes | Composed bytes | Delta | Budget | Composed headroom |
| --- | ---: | ---: | ---: | ---: | ---: |
| Backpack | 1,094,410 | 1,161,783 | +67,373 | 1,343,488 | 181,705 |
| Combined | 13,889,731 | 13,907,572 | +17,841 | 14,155,776 | 248,204 |

Receipt: /tmp/codex-backpack-socket-s1-measurement.json. All eight artifacts
were hash-checked and all four measurement targets are absent. This pair does
not certify the later D1/OKX composition.

Actual D1 and OKX landings composed automatically outside README/CALLERS at
9199ee739573 on702afdffef3. hcm source/API/fixture/map carry is CLEAR.
The sole changed owned fixture combines incoming D1 Identity constructors and
accessors with the cleared two-line first-open/drain; all534 prior assertions
and raw literals remain exact. Subsequent f567d9ac91c merges shrink-only Bitget
budget-check landing bebaa2cc36f automatically. Rust source and Backpack/combined
budgets are unchanged. No authored generic engine or signer change is present.

On9199:377 native PASS,0FAIL,1existing ignored across27targets; all-target
strict Clippy, owned formatting and full bans/ratchets PASS. These Rust gates
carry across the two incoming Bitget budget-check files. Qualification receipt:
/tmp/codex-backpack-socket-d1-qualification.json.

Fresh local builds compare bare bebaa2cc36f with f567d9ac91c using the same
Rust1.99.0/wasm-bindgen0.2.129/wasm-opt132 and unchanged harness commands,
CARGO_BUILD_JOBS=4, BINARYEN_CORES=4, SAFE_CEX_WASM_OPT=required:

| Bundle | Bare bytes | Composed bytes | Delta | Budget | Composed headroom |
| --- | ---: | ---: | ---: | ---: | ---: |
| Backpack | 1,094,881 | 1,162,253 | +67,372 | 1,343,488 | 181,235 |
| Combined | 13,908,680 | 13,926,452 | +17,772 | 14,155,776 | 229,324 |

Web and Node are byte- and SHA256-identical in each row. All four measurement
targets were deleted immediately after their build; sources, logs and artifacts
remain outside Git. Receipt: /tmp/codex-backpack-socket-d1-measurement.json.
The combined row is the required local combined record, independent of CI.
The local Backpack pair is qualification evidence; under the overseer correction,
CI records the family size. Frozen-head CI and exact-tree auditor GO remain.

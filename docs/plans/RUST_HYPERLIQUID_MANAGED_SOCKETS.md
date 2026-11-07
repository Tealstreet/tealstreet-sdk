# Hyperliquid whole-family managed sockets

## Scope and ownership

Base: actual landing `abc72534026dc46fff4a0dd79c9438c8d459c5e4`.
Branch: `feat/rust-hyperliquid-managed-sockets`. Owner: codex-5c57jl.
Coordinate the lib.rs and account.rs socket regions with request owner
codex-zs5e83 before production edits. This plan and original-source tests
precede the adapter. Frozen #1943 stays unchanged until its train lands.

Migrate the entire Hyperliquid socket family: perps, standalone spot and
perps with spot markets, including their private traffic. Every engine instance
has one physical connection multiplexing its native public/private topics.
There is no public-managed/private-legacy variant. Phemex's partial WIP is
parked separately at `254f55689a7`; its account failures demonstrate why a
whole-family boundary is required. Bitunix remains excluded by Sam's decision.

Intended production files: venue Cargo.toml, lib.rs socket fields/hooks and
native subscription projection, a new socket_codec.rs, and account.rs's
private_subscribe projection and its two subscription-to-refresh call sites.
Preserve HTTP construction, account reducers and snapshot revision rules,
managed candle callbacks, history, signing, trading, identity and discovery
semantics. Do not edit #1943's engine/host/OKX files. Existing financial
assertions stay intact; any fixture using duplicate open as a refresh must
instead cause a real reconnect without changing its causal fence.

## Native transport contract

The existing ws() chooses the same mainnet/testnet URL. The channel is Public
only for public_data_only; otherwise it is Private, including a private engine
without a resolved user. Public Direct connects normally; a suppressed public
feed creates no real connection, watchdog or reconnect churn. A Private socket
is supervised under every public-feed policy, but emits/decodes public data only
under Direct. Shared provider prices continue to revalue the native account.

Supervisor policy: connect 10 seconds, no WS authentication challenge, idle
45 seconds, periodic `{"method":"ping"}` every 15 seconds, backoff base
1 second/cap16 seconds with existing jitter and stable reset30 seconds.
Native receive refreshes liveness before JSON parsing, hence AnyText with
ControlPolicy::None. Private subscription payloads contain native user/topic
data; neither an HTTP credential nor a synthetic socket auth phase is needed.

Opening currently sends private subscriptions, starts refresh_account when
account.started/user/private permit it, then emits Direct public subscriptions.
Private topic selection is mode-dependent: standalone spot uses notification,
spotState, userFills and userNonFundingLedgerUpdates; perps adds orderUpdates,
userEvents, userFundings and clearinghouseState, optional portfolio spotState,
four native per-dex topics and activeAssetData for eligible markets. Preserve
the exact optional fields and their order. Mode resolution can happen after
open: mode_ready also subscribes before refresh_spot_holdings/live_balance.
Both sites need an explicit subscription-before-refresh handoff.

Direct public topics are allMids, perps fastAssetCtxs, eligible dex allMids,
retained refcounted book topics and retained candles. Perps books use l2Book
then bbo; spot books use five l2Book variants with native nSigFigs values
null/2/3/4/5. Preserve coin normalization, interval validation and serializers.
Reconnect invalidates the same depth/spot-depth/time/bbo/book state at the
same native boundaries. No financial state is reset merely by a disconnect.

## Shared ownership and handoff

Use one FeedId and its current epoch, declared with the native role/policy.
The shared core owns connect/open/close, expiration, retry, timers and fencing.
Native codecs own JSON and desired topic order only. No raw socket authority,
Supervisor, second timer owner, local deferred queue or release ledger remains.
Desired interests project all active public/private topics with stable canonical
keys. Refcounts stay in the venue's existing books/candles. Mode/market/dex
changes resubmit a complete desired projection rather than an imperative send
that bypasses convergence. Admission/refusal must preserve previously committed
interests and native refresh obligations.

Use #1943's reviewed set_socket_interests_then and ordinary-ingress continuation
for the two refresh sites. Consume its actual landing before certification, or
an explicitly authorized exact reviewed stack during implementation. The
continuation carries only native refresh intent; epoch, canonical interests,
revision, credit, timer and expiry custody remain shared. A retired/expired or
changed feed must never start a refresh from an old handoff. Required private
subscription frames must survive final fencing and precede refresh HTTP; a
queued proposal or a filtered WsSend list is insufficient proof.

Before implementing, settle with the request owner how the existing opening
trace (private frames, refresh HTTP, public frames) composes with complete
desired-interest admission. Preserve private subscribe-before-refresh and
private-before-public wire order; do not silently claim full cross-transport
trace equivalence if all committed public frames precede the later refresh.
Any additional sequencing authority requires shared-owner review, not a
venue-local queue. Native HTTP/reducer publication semantics stay unchanged.

Request-owner coupling review permits the complete private-then-public batch
before the #1943 wakeup and HTTP refresh. Pin the unfiltered returned host
batches and record the moved public/HTTP ordering in CALLERS. This is not an
exact cross-transport trace claim. The continuation selects current native
refresh/inflight/revision state at callback time, never at proposal time.

Only defer when native private subscriptions actually exist. mode_ready must
still synchronously mark mode_resolved and clear mode_timer, then perform its
native refresh_spot_holdings/live_balance when no socket is open or no user
exists. Similarly, opening with unresolved mode still refreshes a known
private account even though private_subscribe has no frames. Do not park these
zero-frame obligations behind convergence or a nonexistent epoch. Add original
working controls for mode before first open, mode while closed and open before
mode, in addition to the full-action private/public/HTTP trace.

## Original REDs and working controls

Commit this plan first, then new real-engine tests against unchanged production:

1. Duplicate current WsOpened sends no duplicate subscriptions or refresh.
2. WsOpened after connect10s, without the watchdog callback, releases no frames.
3. Public allMids after idle45s, without a timer turn, cannot publish or revive.
4. The same expired-ingress fence prevents a private financial frame changing
   positions, leverage or balance. Include a live private control with the same
   body to prove that a rejected publication assertion is not vacuous.
5. Working opening controls pin exact public wire and channel, private topics,
   native refresh ordering, suppressed public behavior and private behavior
   under Shared/Disabled. Include standalone spot and merged-spot fixtures.
6. Actual close/reconnect retains topics once, ignores stale old ingress and
   keeps book/candle refcounts. Native ping/backoff/idle policy stays executable.

After adapter adoption, add final-handoff controls through real timer ingress:
both reconnect and mode-ready paths, no refresh before required frames, stale
continuation inert, and native private publication after a successful handoff.
Keep the original full-action assertions. Adapt fixture setup only through real
accepted opens/auth/reconnects; never turn a duplicate open into a reconnect.

## Verification and inventory

Run the dedicated controls first, then scoped Hyperliquid native targets,
strict Clippy/fmt and ratchet checks. Request an independent protocol/request
coupling review and source preservation review. Rebuild full CI with unchanged
Hyperliquid public/private/spot replay CPU, retained-memory and size objectives.
No profile, reference, threshold or gate adjustment. Native test success is
not a performance claim. No merge without the explicit engine/train window.

At this base, socket-legacy-sites.json contains two lib.rs fingerprints:
`974264f5bfd3c268b4c30755e61c8269f51a1cd01ebaf1e6a51c77a8e58659cb`
(2 occurrences), and
`fd78adb5b21889b836207c7d212d4694d9e34d6bfeb0e5f19023c0f2abffd7c3`
(1 occurrence). Verify the scanner and remove only actual retired entries;
the family sourceRoot can be removed only when no legacy socket site remains.
All other family maps stay exact. The planned reduction is2F/3O, not a claim
that the current77F/90O burn-down has already decreased. Add landed-behavior
CALLERS text with the implementation, keeping proof/status in the PR body.

## Original-source checkpoint

On unchanged `abc72534026` production, the dedicated managed_socket target
compiles and runs 8 tests: 5 genuine assertion REDs and 3 working GREENs,
zero ignored. Duplicate public and private opens replay work; late open
releases frames; idle-expired allMids changes100 to123; the identical expired
private state changes contracts 2 to 3. Live private state and retired private
state controls distinguish accepted publication from inert old ingress.
Exact native opening public JSON also passes. No adapter or feature opt-in
was present for this run; this is not a CPU/memory or full-suite result.

After plain integration of actual `a828ede7ebb`, the Hyperliquid production
source is still byte-identical. Three additional original working controls
pass: Shared/Disabled private financial ingress, standalone spot's five exact
depth variants, and the full-action private reconnect subscription-before-HTTP
order. The resulting run is 5 assertion REDs and 6 GREENs, zero ignored.

The mode-boundary closure adds 3 more native working controls: mode response
before first open, mode response while closed, and open before mode response.
Each proves its no-private-frame refresh obligation remains synchronous/native.
The first fixture run omitted maxBuilderFee; adding that native response fixed
the fixture without touching production. The complete run is now 14 tests:
5 original assertion REDs and 9 GREENs, zero ignored. The reconnect control also
pins the original unfiltered private/HTTP/public order before its explicitly
reviewed transition to the shared handoff ordering.

Ownership is coordinated: zs5e83 has no active lib/account claim; fe7lgm owns
history.rs and only its Pending/callback/pump_reads branches, which stay exact;
19pt8s confirms no socket-region claim and requires both existing wallet-check
sites/outcomes to remain exact. Native coupling reviews of the original plan
and controls are bounded read-only clearances, not production certification.

## Adapter checkpoint

The plain prerequisite merge is `12cc6044e5f`, with frozen #1943 `2f7102ac0e3`
as its second parent. Its engine, host and OKX blobs are retained exactly. The
only merge resolutions retain both independent shared-doc additions. No seam
source is edited by this adapter; actual #1943 remains the landing dependency.

The native family now uses one shared FeedId and its current epoch. The remaining
open bit selects native topic eligibility and is updated only by accepted shared
Ready/Retired events; it owns no transport admission, retry, timer or identity.
Private/public JSON projection supplies desired interests, and the wire encoder
retains private-first ordering and the five spot depth variants. Both refresh
sites use shared handoff custody when private frames exist, and native immediate
refresh/publication when no private subscription obligation exists.

All 241 scoped Hyperliquid tests pass across 17 targets, none ignored, including
16 dedicated socket controls. Five original ingress REDs become GREEN; the
original private/HTTP/public trace remains identifiable at `d8bc46e7ca5`, while
the adopted assertion checks full returned batches through the actual wake.
New mode controls pin synchronous timer retirement, zero-frame refreshes,
post-handoff balance publication and stale mode wake inertness.

Existing account/trading fixtures that replayed duplicate opens now drive genuine
close/retry/replacement/open/handoff transitions. They consume actual jittered
retry timestamps, retain the evolved physical socket, and sequence repeated
snapshot fixture clocks monotonically. Tight post-reconnect ingress clocks move
by 1 second with their existing timer expectations. Financial assertions and
wire bodies remain unchanged. One startup fixture now opens its real socket
before delivering its private financial frame; pre-open ingress is fenced.

The socket scanner removes exactly Hyperliquid's 2 fingerprints/3 occurrences
and its family root; every other family entry remains exact. Native source,
strict and preservation review evidence is separate from rebuilt CI's CPU,
retained-memory, size and replay gates. No native result is a performance claim.

## Review regressions and correction

Request-owner review of `3b261bf21b2` found two concrete issues. The executed
test-only checkpoint `ae9c1205a64` has 2 assertion REDs and 16 GREENs: mode
completion loses its spot read after genuine shared interest-count refusal,
capacity freed and fresh reconnect/handoff; final perps book removal emits bbo
before l2Book. The spot five-variant final-unsubscribe control passes on that
same source. Wire fix `8cd6ed75ac7` restores native removed-key ordering and
passes both ordered-wire controls. Additional RED `ecc147faca5` proves that
retiring admitted mode work also loses its obligation after a new handoff.

Uufub4 clears an intent-only native mode_refresh_due state with shared recovery,
not a venue queue/timer/epoch. Fix `6a3f9dda954` preserves it across refused and
retired handoffs. The native mode state/timer still change synchronously. Matching
current-feed capacity refusal uses shared retirement/backoff; Ready and ordinary
open interest changes register fresh shared handoffs for still-due work. The
validated callback first hands spot work to the existing native read owner
(including its quota queue), checks native inflight ownership when required,
and performs native balance publication before clearing intent. A late old wake
cannot consume the intent while its fresh handoff is pending; duplicate old/new
wakes cannot issue the read twice. No shared source or signature changes.

All 243 scoped native tests across 17 targets pass, including all 18 socket
controls; strict all-target/all-feature Clippy, scoped formatting and bans pass.
The two new zero-frame balance/timer assertions also pass against isolated
pre-adapter `12cc6044e5f` production in a cold target. Final review and fresh CI
apply to the corrected head; earlier CI remains historical evidence only.

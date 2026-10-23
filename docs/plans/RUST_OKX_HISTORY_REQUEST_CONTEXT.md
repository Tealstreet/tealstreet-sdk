# OKX history RequestContext adoption

Plan first, 2026-10-24. Owner: codex-ih35js, handed over by claude-4yjlw0.
Initial planning checkpoint `aa556d59cd904fe6cd81f489fc7680f80cbe578c`; branch
`feat/rust-okx-history-context`. This phase commits the inventory, plan and
original assertion REDs only. No venue implementation or lane-owned inventory pruning until actual
#1945 lands; no merge without an overseer window. #1856 remains frozen for
combo-4b at `774a27026888bbfca080f2605e957e631c388bb1`.

## Handover and collision boundaries

claude-4yjlw0 confirmed this history.rs handover and supplied the native policy
and signer-chain notes before edits. It keeps Bitget and OKX callers #1938
(leverage/config/trade entry) and #1942 (candles). #1773 supplies signing and
currently owns history/lib/sign/closed and existing fixtures. #1943 also owns
lib/socket regions. These source/test files are not edited in this phase.
Only this plan, its index row and a new standalone history-control file change.

The branch began at aa556 for original-source evidence. Current landing may be
plain-merged as it advances; no rebase is needed. The handover recommends future
implementation after caller-chain head `b09e8882187` containing #1773/#1938/#1942
or their actual landings. Re-integrate current landing and the actual signer/
caller landings and refresh claims before code.
The history adoption consumes #1773's dispatch recipe through
`request_signed_managed`, ordinary signed GET selection Secondary else Primary,
with client-ID lookup explicitly Primary. It adds no signing implementation.

On that chain, three lib.rs sites remain tied to history: list (b8eef2a7),
sign_http (2e921890) and History's on_sign_error terminal (c873db6f). After all
consumers are accounted for, coordinate their exact prune and deletion of the
#1773 STAGING_BOOTSTRAPS entry with 4y. They are conditional follow-up scope,
not part of the exact eighteen-call promise below. Do not edit held lib.rs now.

## Exact inventory map

At aa556, OKX has 35 fingerprints/45 occurrences: account 6/6, history 14/18,
lib 6/8 and trading 9/13. Full history.rs adoption removes 14/18, projecting
whole OKX to 21/27 before any later funnel prune. The scanner verifies all
eighteen are genuine calls; no macro-only occurrence is claimed. The immediate
local-only subset is 7 fingerprints/9 occurrences, not the whole target.

| Fingerprint | Occurrences | Native branch | Shared destination |
| --- | ---: | --- | --- |
| `219d41f6382035ac32a6f3d3b0656b435121ade8f20af51127cd7f617db9334e` | 1 | Serialized history success | Original Read-root completion |
| `d58bbdc6877786ed4f458e3ab566d623c3af774cc2bf29627cada3d35937483a` | 1 | History serialization failure | Original Read-root failure |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 3 | Missing credentials, missing Fills symbol, unknown market ->[] | Genuine local Read entry/completion |
| `2ef56c72a7cc604e392db93ffd88ff38bb627358b6784a3259ce2254d38432c2` | 1 | Invalid history bounds | Genuine local Read entry/failure |
| `ef65cf51343dbe5aca475bfd9ec4ce620ac65579e8f0b6e52d935f82437e7275` | 1 | Invalid client ID | Genuine local Read entry/failure |
| `896c2dc203c518dc9ae1c6b5eb51b5591da28af19f9ff2461c4c629a490fb1a5` | 1 | Unknown lookup market | Genuine local Read entry/non-definitive miss |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Cached lookup success | Genuine local Read entry/completion |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Cached serialization failure | Genuine local Read entry/failure |
| `6bdc21c11d671d604715e4720a738131cd6c659c4c02b2aef436013584f03520` | 1 | Lookup credentials unavailable | Genuine local Read entry/failure |
| `9fbbf1645dbf2e85625a23bf00328e2e4b9e0783f83d256e85b0628f56ea1588` | 1 | Primary signed lookup GET | Managed Read prepare/bind/claim |
| `755a658b5cfaa9ec659ee94f62b5f535e94a4a91391476bda23ef1199af846bf` | 2 | Lookup result; query failure ->[] | Original Read-root completion |
| `d5e898e07cc7c2b9f7927c0f9a7f462214e004fdec265ff4f3ed0370a6db1a81` | 2 | Lookup result/failure errors | Original Read-root failure |
| `3c8b186aad2675ec860322e1e33c2976d18c43ef5c7f04269b5de3a84ab8ff6d` | 1 | EEA composite page cap | Original Read-root failure |
| `799a14889bffdec46d7a35b3067158566a1b2cbaa0e0db6c31a23dfff38ebbc5` | 1 | EEA composite cursor stall | Original Read-root failure |

Closed positions (`FetchClosedPositions`, closed.rs, fetchClosedPnl/okxJournal)
are a separate source and are not part of these eighteen sites. Existing closed
position tests remain useful compatibility gates, without a conversion claim.

## Root and physical-page contract

The ruling is one immutable root equal to the caller's effective
`rustCommandTimeoutMs`, including a per-call override; default 120000 ms.
#1945's owner confirmed the superseding API:
`Cx::command_wait() -> Option<CommandWait::{Remaining(u32),Expired}>`,
not the earlier proposed command_timeout_ms name. Reviewed plan pin
`de14dd30926`; original core RED checkpoint `dbdcee8c524`. Source is not frozen
or landed. Do not copy receipt/deadline storage into this venue.

Capture Remaining only in the genuine entry and admit the Read root once with
that remaining duration. The root is immutable and bounded by the original
receipt deadline after held entry; Remaining floors whole milliseconds, so a
fractional hold can make it under1ms earlier, never later. No page or fallback
renews it. Fresh Expired needs shared typed Timeout rejection before any effect,
Read outcome None. Missing entry custody grants no work. Initial entry refusal is
already published by the engine; never add a numeric fallback. The #1945 owner
proposed `Cx::reject_expired_command(Operation)` at `b0d875eff64`, taking no
numeric ID and using the shared entry rejection transaction. It grants nothing
unless the exact Fresh entry is Expired. Source review and actual landing remain
required; do not fake zero-timeout issuance or implement receipt logic locally.

Each physical page gets a Read child of that root with timeout 20000 ms and
RetryPolicy::Never, minted only when the native queue dequeues it, before
managed route preparation/signing. Its deadline is capped by the root. The
root covers all native 100 ms pacing waits; the child budget starts at dequeue,
so a waiting page cannot expire before it is minted. Preserve the existing
100 ms cadence only where native orders/income continuation already waits.
Initial calls, fills-to-fills-history and EEA SWAP-to-FUTURES transitions stay
immediate, each minting a fresh child at its actual send/dequeue boundary.
No new venue queue or cross-cutting timer implementation. At an observed expired
root, stop new pages and discard staged query rows. There is no automatic root
timer: in-flight child timers are root-capped, while queued work is cleaned up
when its existing pacing timer wakes. Do not claim exact-at-root queued cleanup;
that would require a separate shared seam.

Preserve the native post-release terminal projection: command_error
`OKX history: {error}`, then soft[] for ordinary transport/HTTP/parse query
failures; lookup rejects and never becomes definitive absence on failure.
#1773 provider/signing-preparation refusal is a separate strict typed NotSent
failure for both query and lookup, with no soft[] or command_error. Preserve
every typed field through root failure. Root expiry follows soft-query behavior
only after an actual release; issued pre-release expiry remains strict NotSent,
as in #1942. Fresh Expired before any Context is the distinct shared initial
Read refusal above (outcome None). Obtain release evidence from shared lifecycle
contracts; no venue status ladder or fabricated release flag. Existing
completion-only root validation permits settlement after deadline without send
or DATA authority; shared API-fit review must confirm the full composition.
Cap/stall semantics stay separate: plain history marks its own incomplete result
and retains rows; EEA composite cap/stall rejects. Root expiry must not publish
staged partial history or borrow another command's marker.

uufub4 confirmed API fit: original Context root plus http_leg_context is
supported for sequential uniform-credential Read work with no mutation or join;
Plan/1928 is not intrinsically required. Use ReadCompletion for native terminal
shaping and validate_delivery/guarded row admission before staging. Do not use
ReadHistoryRows.finish(Err) to replace native soft[] with retained prefix plus
incomplete. Always derive children from the original root, not a prior child;
queue validation precedes selection/build/send. This clearance supplies no
caller-wait propagation until actual #1945 and does not clear source collisions.

## Native data and caller invariants

PAGE100; orders cap50 and income cap200 per product; EEA unscoped queries walk
SWAP then FUTURES before the global result. Fills use up to two calls: recent
fills, then fills-history only when native count requires fallback. Bills choose
bills-archive when the requested lower bound is older than six days. No retries.
Exact decimal ordId/billId cursors, last-wins identity updates, sorting, filtering,
contract/base conversion and bill fee/PnL/residual decomposition remain native.

Credential absence precedes ordinary query validation. Fills normalize seconds/
milliseconds and drop nonfinite bounds; Orders/Income reject nonfinite or
reversed bounds before unknown-market empty results. Zero limits clamp to1
for Fills/Trades; Orders/Income zero means unbounded. Invalid client ID precedes
market/cache; a secondary read key disables lookup cache use. Keep UNKNOWN
non-definitive, and ABSENT definitive only for native51603 on successful status.

Consumers: fetchHistoricalFills, fetchHistoricalOrders, fetchIncomeHistory,
fetchIncomeHistoryRange, fetchPublicTrades and findOrderByClientOrderId. Caller
root expiry now bounds total work without changing the host's abandonment
error. Future CALLERS must distinguish fresh typed admission failure from native
post-release soft-query/reject-lookup behavior, strict typed provider/pre-release
refusal, and explicitly record shared
ReadCompletion's Internal-kind enrichment of otherwise untyped native errors;
never strip typed provider/native metadata locally to mimic old kind=None.

## Original REDs and phase gate

After independent plan review, commit a new standalone real Engine<Okx> control
file separately, leaving production, existing fixtures and inventory exact to
aa556. Reuse existing native history/lookup/closed and signer-chain controls.
The pre-#1945 explicit-wait fixture adapter transparently uses legacy handle
without propagating the requested wait, as the core's original RED checkpoint
does. It grants no fabricated Context and makes no caller-wait readiness claim.
After actual #1945, replace only that adapter with real handle_command; retain
all behavioral assertions and re-prove which failures belong to venue adoption.

Required assertion REDs: genuine local admission at nonfinite time; managed
public/private/primary-lookup transport; short caller root expiring during the
100 ms page wait; default120s expiry across individually valid20s pages with no
root renewal. Compatibility controls: native soft[] plus command_error after
malformed/failed later page, rejecting lookup, finite local precedence, exact
Primary/Secondary wire, and a20s child born at dequeue (response after enqueue
+20s but before dequeue+20s remains eligible). Add a longer explicit caller wait
control to forbid an unrelated fixed120s root. Later integration must add held
entry (fractional hold and sub-ms expiry included), exact boundary, root-capped
child, prefix rejection, provider refusal (query and lookup, every typed field preserved, no soft[] or
command_error), pre-release versus post-release expiry, duplicate/late/reused
owner and same-command typed expiry controls.

Report assertion failures and passing controls separately; no compile failure or
ignored test is a RED. Stop after plan+original RED commits until the shared and
ownership gates clear. Implementation, exact prune, full focused OKX/Binance
checks, strict Clippy/fmt, both ratchets, three independent reviews and full CI
follow later. No current inventory reduction or managed-history readiness.

## Original-source receipt

Plan `dc17d83c934` precedes API-fit correction `64150f5746d` and original test
commit `6569540b7f5`. Fixture-only correction `c4be3f3420d` scopes dead_code
allowance to the imported shared support module; the first compile failure is
not an assertion RED. On unchanged aa556 production, the qualifying run gives
7 assertion REDs, 5 compatibility passes and0 ignored tests.

REDs prove missing managed private/public/primary-lookup preparation, invalid
entry bypass by a local empty answer, short-root expiry during native pacing,
default120s expiry across eight individually valid pages, and expired lookup
returning native51603 absence. Five positives prove finite precedence/sentinels,
soft failed later query page and strict lookup transport failure/Primary route.
They also establish native response eligibility after enqueue+20s but before
dequeue+20s, and native130s completion with an unpropagated180s fixture argument.
They do not prove child construction or override propagation on this baseline. The explicit
wait adapter deliberately lacks core propagation; these assertions must be
re-proven with actual #1945 before attributing remaining failures to adoption.
Secondary-wire/provider behavior is not newly proven by these five controls.

Source/adversarial, caller and full-map reviewers independently verified the
original-source boundary and test reachability. At that original checkpoint,
production, engine, inventories, ratchets and existing fixtures were byte-exact
to aa556. The
phase is intentionally RED and not merge-ready. No legacy site is pruned.

Strict all-target OKX Clippy and fmt passed for that original-control
checkpoint; checker tests passed15/15. Its provenance check refused the older
manifest because incoming Paradex history entries were pruned after aa556. That
was not a current-landing ratchet PASS; the branch had introduced no growth.

## Current landing integration

Plain-merge actual `a828ede7ebb54bc9c032ea61365213abac5e3dca` to carry incoming
source and all inventory prunes. The merge is conflict-free. Only this plan,
the README index row and the original new control file belong to this lane;
all production, prior tests and the manifest equal current landing. The
historical aa556 original-source receipt above remains intact. Recheck both
ratchets and the unchanged controls at this integration; implementation stays
held for actual #1945 and source ownership. No inventory prune or managed
history readiness is claimed.

## Reviewed command-wait implementation prerequisite

The overseer authorized implementation against frozen #1945
`15df705c0b52efd2b9963b2f8980d6c4c947b954` before its combo-5 landing.
Actual landing `15bd7821b52` and that frozen head are now plain-merged; no
shared source was changed while composing them. The real API is
`command_wait`, `reject_expired_command` and `Engine::handle_command` as
described above. Earlier statements that source is not frozen describe the
original RED phase. Final integration must consume the actual #1945 landing.

Refresh the original controls against real `handle_command` propagation before
attributing caller-root failures to venue adoption. Keep the original RED
checkpoint intact. Invalid host input is a distinct API boundary:
`handle_command` returns typed Invalid without dispatch, while plain `handle`
can enter with no receipt stamp. The getter itself grants no authority; never
fabricate a receipt, substitute a default root or emit a numeric terminal.
The overseer's correction below settles the None boundary.

The fresh collision check still lists #1773 history/lib source and #1943 lib
source. The #1773/#1938/#1942 chain and narrow source handoff are being
reconfirmed with claude-4yjlw0. No held venue source is edited until the agreed
stack/order is established. Planned history shrink remains 14 fingerprints,
18 occurrences; the conditional three-site lib funnel prune remains separate.

On this composition, valid finite receipts now use real `handle_command`,
including explicit 250 ms, 1000 ms and 180000 ms waits; the nonfinite-clock
control still exercises plain handle's separate unstamped entry. The unchanged
behavioral assertions produce seven runtime assertion REDs and five native
compatibility passes, zero ignored. These are venue-adoption gaps with actual
caller-wait propagation, except the separately reviewed unstamped-entry edge.

claude-4yjlw0 reconfirmed the reviewed chain `e5aea380675` (#1773),
`2a58ec8d414` (#1938), `b09e8882187` (#1942), with no successors. History.rs
ownership passes to this lane from b09e8882187; its signer edits are final on
that chain. The only permitted lib.rs edits are Pending::History response,
failure and typed signing-failure arms. Send/sign funnel pruning and deleting
the OKX staging entry wait for a later coordinated slice after actual landings.

The overseer corrected the earlier seam-owner suggestion recorded in local
`616f7ca3ba0`: None must fail closed through the shared typed Read entry refusal
in fe7lgm's #1945 follow-up, with outcome None. It must never fall through to
DEFAULT_COMMAND_WAIT_MS: #1945 already stamps the SDK default when the host
omits a timeout, so None denotes receipt failure, not an absent override.
Remaining supplies the caller-bounded duration; Expired uses the shared expiry
refusal. No venue-created root or receipt substitutes for either failure.
The original NaN-clock local-empty assertion stays unchanged. A new finite
clock control uses f64::MAX, where the default receipt deadline cannot advance,
and demands one typed Read refusal without a local empty success or I/O.
The explicit host-boundary handle_command failure remains a separate typed Err
with no dispatch. Implementation of None awaits the reviewed public refusal API.

## Reviewed OKX chain composition

claude-4yjlw0 supplied CI-green composed #1942 `0095ef76ac7`, including #1938
`935167475e9` and #1773 `486b7c04302`, on actual15bd. Plain merge fdbdde063a3
consumes it alongside the earlier frozen #1945 15df. There are no source
conflicts; CALLERS retains incoming chain bytes followed by the exact #1945
suffix. All OKX production files equal the reviewed chain. The earlier failed
b09 composition was aborted without any shared-source guesses.

The original target on this composition compiles 14 tests: eight assertion REDs,
six compatibility passes, zero ignored. This includes the added None-receipt
RED and passing host-boundary refusal control. Both request-context checks pass,
including 15 checker controls. No production adoption or prune is claimed.

The overseer directed the next integration to #1945 `c0658f9ed97` once frozen.
Wait for that reviewed/frozen source and its public receipt-refusal API before
implementing None handling. Keep all original assertions, typed Read outcome
None for initial refusal, and the no-default-substitution ruling above.

## Updated composed chain checkpoint

The signer owner supplied reviewed, CI-green #1942 head8e212d4f8c9, composed
from #1938 718deab3bd0 and #1773 a805750f64d on landing fade3aa0a41.
Plain merge8c0a5cedc55 integrates it without source conflicts. The only conflict
is the README index: retain the incoming handoff row first and the caller-wait
row once. All nine OKX source files are byte-exact to the incoming pin; the
history target is byte-exact to the prior checkpoint bf94706823b.

The unchanged target compiles and executes fourteen controls on this composition:
eight expected behavioral assertion REDs, six native compatibility passes,
zero ignored. No history production implementation or inventory prune is
included. The composed chain's CI is dependency evidence only, not a green
claim for this original-RED target. Missing-receipt handling remains held for
the reviewed public refusal/root-expiry API; no default-root substitution or
venue-local receipt shim is introduced.

## Tracked caller-root implementation contract

Actual OKX signer/caller landings through0418d0a4599 are genuinely integrated
at27f687b3c9f; provisional reviewed #1976 0a35dc34dee is plain-merged locally
at4bd3b9fccb7 with shared documentation keep-both resolutions only. Per the
overseer's current root-expiry ruling, this supersedes the earlier untracked
root description: admit_caller_read_root captures the exact raw receipt deadline
and the engine expires it strictly, discarding all rows, markers and native
finalizers. This applies both before and after release. No venue release flag
or root timer is introduced. Root retirement only removes this exact Owner's
delayed history continuations; leaked native100ms pacing timers become inert.

The two original expiry controls remain available in their original checkpoint.
At this revised contract they must demand strict Timeout/outcomeNone/no rows
from the shared funnel, rather than the earlier planned soft-empty expiry.
A separate original-source assertion RED pins root-timer expiry while parked,
without firing the native pacing timer. Ordinary live-root released HTTP, parse,
transport and child-only failures retain their native command_error plus soft[]
for queries; lookup stays strict. Provider/preparation refusal is always strict
typed NotSent for both, never definitive absence. Missing receipts are refused
by shared admission, never substituted with a default.

Each native send/dequeue derives a20s/Never child from the same original root;
ordinary signed reads keep Secondary else Primary, client-ID lookup Primary,
public trades no credential. Recipes and opaque identity come from actual#1773.
ReadCompletion validates DATA and owns finalization; native cursor/dedup/row
codecs remain unchanged and ordinary failure discards all staged rows. Only
Pending::History lib hooks are edited; shared list/sign funnel still serves
discovery and cannot be removed without an exact remaining-consumer audit.
Implementation is local until the replacement frozen #1976 composition is
announced; no mergeable PR/READY or source conflict guesses.

The revised test-only checkpoint executes15 real-engine controls on unchanged
OKX production:9 assertion REDs,6 native compatibility passes,0 ignored. The
new parked-root assertion deliberately observes the absent engine caller-root
timer; native request/timer setup is otherwise supplied by the passing page
pacing controls. The two expiry assertions now demand the current strict engine
Timeout contract, with their original versions retained in Git history. No venue
source, manifest or existing financial oracle changes in this checkpoint.

Plan review tightens expiry controls to count every correlated terminal and
forbid INCOMPLETE markers. A pre-release child expiry/preparation error arrives
through the shared strict signing/preparation failure callback; a released child
timeout through typed on_http_failed retains native live-root query soft[].
Synchronous child mint or managed submission refusal rejects strictly through
the root and cannot soften accepted rows. No release inference is added. Primary
managed routing uses explicit Prepare intentPrimary; a physical routeNone means
the host's normal primary egress, equivalent to native explicit Primary. A new
adoption control pins that intent before signing.

## Local implementation checkpoint

The15 original/revised controls now pass, with five additional adoption gates
for20 total: managed Primary intent, unreleased strict versus released soft child
timeout, typed preparation metadata across query/lookup/public, strict paced
child-capacity refusal preserving peers, and retired pacing/late response after
command-ID reuse. The earlier failing immediate provider fixture was unsupported
for parked signing; the native provider-refusal oracle now explicitly completes
route preparation before dispatch signing and retains its NotSent/no-absence
assertions. A new-control Rust pattern compile error was corrected before the
20-control PASS; it is not counted as an original behavioral RED.

The existing17 history/8 lookup/5 signer tests pass after shared prepared fixture
glue and monotonic history response clocks. All financial/identity/signature/
lookup assertions remain; physical routeNone is normal managed Primary routing,
with explicit preparation intent pinned separately. Native queues/codecs and
PAGE/cap geometry are unchanged. Full source scan removes only history.rs14F/18O
and lib.rs History sign-failure1F/1O. The retained2libF/2O and staging exception
still serve discovery, confirmed by claude-4yjlw0. No engine or signer source is
edited, and no replacement frozen1976/CI/READY claim is made here.

Complete scoped verification passes573 OKX/Binance tests across45 targets,
including all20 dedicated custody controls, native closed positions and product
fixtures. Five product-history fixture paths now drive shared preparation at the
actual event clock and select the one physical HTTP among timer actions; their
financial/product assertions are unchanged. Strict all-target/all-feature OKX
Clippy and rustfmt pass. Clippy's enlarged-variant finding is resolved by boxing
only Pending::History; no other pending/reducer representation changes.
The local scanner exactly matches the manifest and the checker suite passes15.
Full provenance currently rejects only stale incoming Aster/BingX entries on
origin; genuine replacement1976 integration must bring those foreign prunes.
No foreign map, scanner or engine source is edited. Independent implementation
reviews and fresh exact-composition CI remain outstanding.

Independent caller review corrected one documentation grouping: asynchronous
pre-release provider/preparation failure is NotSent, while synchronous child/
submission failure keeps the shared Read projection (outcome None). Source was
unchanged. Source/adversarial review requested stronger peer preservation proof:
the capacity test now retains every genuine peer Preparation and verifies each
remains pending before and after the owner refusal, while forbidding new Prepare/
Bind. All20 controls and focused strict Clippy still pass. These are review
follow-ups, not additional original REDs or a runtime change.

Independent source/adversarial, caller/fixture and full-map reviews are CLEAR
for source2bdce833c21 and the reviewed documentation/test follow-ups through
1f7547d78c7. Architecture gate: No per-venue implementation of a cross-cutting
concern; this adapter consumes shared tracked roots, expiry, Context completion
and existing dispatch recipes while preserving native codecs and pacing.
Full-map review confirms exactly12 intended paths and all foreign source/maps
unchanged. The signer owner reserves the optional OKX staging-bound tightening
for a separate tiny PR after #1950 lands; frozen #1915 holds the checker files,
so this consumer leaves them untouched. Replacement1976 integration, fresh full
provenance and CI are still required before READY; no landing merge is authorized.

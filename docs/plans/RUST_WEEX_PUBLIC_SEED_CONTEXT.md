# WEEX public discovery and ticker-seed Background reads

Plan first, 2026-10-25. Owner codex-uufub4; bounded plan/native reviewer
codex-hcmq3f. The overseer reserved the WEEX lib.rs read paths for this slice.
Branch feat/rust-weex-public-seed-context starts at actual landing
c5d7c3595af7c9ab189df6e386df1f872466b4a9 (tree8a93c750656). This
checkpoint changes only this plan and its index row.

## Scope and native behavior

Manage only the three unsigned startup/discovery requests in WEEX lib.rs:
Markets (/capi/v3/market/exchangeInfo, streamed symbols), Daily
(/capi/v3/market/ticker/24hr, streamed root), and BookTickers
(/capi/v3/market/ticker/bookTicker, streamed root). Preserve GET, native URL,
empty query, no headers/body/signature/credential and actual bound route None.
Declare Egress::Primary explicitly so a keyless policy does not silently select
PublicCors or acquire a public-proxy routing contract.

The current get() helper also serves subscription Depth snapshots. Those,
command depth queries, candles, private account/history/trading/signing and
credential/session selection remain outside this proposal. Keep only the
existing depth use of the legacy helper; do not add a fallback for the three
adopted task kinds. A task-kind dispatch is a direct per-caller cutover.

Markets currently publishes its nonempty valid catalogue then queues two
independent ticker reads. Daily and BookTickers stage native rows and bit masks;
only both completed and at least one valid Daily row publish ticker state.
Book-only rows remain excluded; bid/ask precedence and malformed-row notices
stay exact. Preserve discovery backoff, ticker retry generation, native429 pause,
private startup even on ticker failure, websocket startup and valid-row retention.

## Authority, ordering and failure custody

At eligible dequeue, after the existing rate-pause gate, mint one genuine
Background Read Context per physical request: credential None,20,000ms,
RetryPolicy::Never, no pacing, explicit Primary egress. Nothing is minted while
paused; the existing deferred timer retains only native unissued task state.
No aggregate root, retry, nonce, signature or Plan is introduced.

Pending retains the exact Context for these three task kinds only. Require
exact DATA delivery before any streamed-row decode, seed staging, skipped-row
notice, global status/rate effects or final publication. Preserve the existing
ticker seed_generation fence as an additional condition. A stale/expired
response has no native effects. Failure-only retirement can settle an expired
physical request and invoke its existing native discovery/ticker failure path.
No stale data may be accepted merely because cleanup owns the request. An old
ticker seed generation is cleaned up without failing or publishing into the
new generation. Only the current generation runs native failure follow-ups.

On success keep the Context live through Store/seed publication, finish that
exact Context once, then run discovery success/backoff, websocket/account startup
and any successor HTTP admission. Markets must retire before either ticker
request queues. Do not reclaim a current Context before its data effects.
On empty/invalid/status/network/preparation refusal or physical Timeout, retire
once before native failure/backoff/account-start hooks. There is no inline
transport retry or auth/deactivation inference. A subsequent native backoff is
a fresh physical request and fresh20s bound, preserving the existing cadence.
Typed causes must not be projected into Network to gain retry permission.

Nonfinite clocks (NaN, +Inf and -Inf) have a separate denial path. Check clock
finiteness before the pause/defer gate and before invoking any native failure
follow-up for these adopted task kinds. An invalid-clock admission returns
without creating custody, a timer, HTTP, socket or private-start work. For an
issued request returned by preparation or transport failure at a nonfinite
clock, retire/drop only that exact issued custody, with no DATA decode, row,
status, diagnostic, seed, rate, Store or readiness effects and no NEW timer,
HTTP, socket or account startup. Existing unrelated finite work stays live.
Cleanup cancellation or clearing the issued request's existing timer is allowed;
no replacement timeout/backoff may be created. This must not suppress genuine
finite owned Timeout recovery: at finite physical-timeout equality, retire the
current request and run its native failure/backoff/account-start path exactly
once, even though DATA validation has expired. Old ticker generations remain
cleanup-only. A later eligible finite observation can attempt normal startup.

## Original controls before runtime

First add a new public_seed_context target against unchanged production.
Real Engine controls must discriminate eager Action::Http from managed
Prepare/Bind/Claim for all three native endpoints, pin absent signing, exact
Primary intent and final route/header/body/stream bytes. Positive fixtures
retain native catalogue/ticker output and bid/ask precedence.

Use genuine256-peer Background pressure, with no fabricated contexts or IDs.
Pin admission refusal and all-credit recovery. Add NaN/+Inf/-Inf admission,
released-request failure and preparation-failure controls for each adopted task
kind; assert no new timers/HTTP/sockets, no native state/diagnostic effects, exact
owned cleanup and a working unrelated finite peer. Keep finite Timeout/backoff
recovery as a distinct positive so invalid-clock handling cannot mask it. For Markets completion,254 peers
must permit the two successors only after its own credit retires. Pin ticker
success/failure retirement before account/socket/native-backoff successors.
Exercise physical TimerFired equality at20s, held preparation, late streamed
rows/final responses and stale ticker generations; require no mutation on DATA
refusal and usable unrelated peers. Pin malformed/partial catalogue and book-only
ticker behavior, current429 delay versus stale429 inertness, no admission while
paused, native exponential discovery retry and ticker-failure account startup.

Keep original public/startup/account/depth assertions and payloads unchanged.
Any fixture adoption uses real prepared_actions at original event clocks. If an
old test crosses the new20s physical bound, retain that original assertion and
record the timing conflict before proposing an explicit fixture correction.

## Inventory, review and validation

The shared raw get helper remains for NONSTREAM Depth only. All current
get(Some(stream)) production callers are the three adopted task kinds; remove
the now-orphan raw request_streamed branch after their direct cutover. Remove
the unused get stream argument and DeferredGet stream field where proven dead;
the candle deferral initializer merely drops that unused field, while its replay
still calls send_candle_query unchanged. Preserve the surviving raw nonstream
Depth request tokens and behavior exactly; no Depth custody adoption is claimed.

The canonical raw streamed fingerprint is
067aa233d0ebbb24c837ae5cb4549d6aa68e9f29bf4bdde0ba0630bda9f466c1,
count1 in venues/weex/src/lib.rs. Recount actual source before source freeze and
remove ONLY that singleton from the production manifest when absent. On this
base that is WEEX16F/22O to15F/21O; no sibling or foreign prune. Keep the raw
nonstream sender, STAGING_BOOTSTRAPS, signing/credential inventories and checkers.
Source scope is lib.rs plus a new test target, narrowly needed fixture drains,
and plan/index/CALLERS docs. No engine/host/Cargo/profile/budget changes.

Commit plan before original controls, source only after bounded API/native plan
and original review. Run scoped originals/new/public/startup/account/depth,
strict venue all-target Clippy, fmt and complete bans/ratchets. Measure composed
WEEX and combined artifacts at the FIRST SOURCE COMMIT, paired against its
bare actual base with the same pinned toolchain and landed profiles. Require
Node/Web equality and unchanged budgets; delete each scratch target after
measurement. Re-measure on a later composition when engine/feature/build input
overlap requires it under the auditor rule. One freeze
push/PR-triggered run after exact source and preservation review. Exact-tree
8x5tm1 GO remains required for landing.

## Bounded plan correction

Reviewer codex-hcmq3f cleared native/API direction at b9673d1ba86 and requested
these two P2 corrections before originals: invalid clocks must not feed native
backoff/private startup, and the orphan raw streamed branch must be removed and
recounted rather than retained for a zero-prune claim. This successor changes
only the plan; originals and runtime still await bounded plan clearance.

## Original-control checkpoint on unchanged production

The corrected plan70b99124bb4 is bounded full PLAN/native/API CLEAR from
codex-hcmq3f. A new public_seed_context integration target now compiles and runs
42 controls against unchanged production:34 intended assertion RED,8 GREEN,
0 ignored. Nine of those REDs stop at the explicit missing-Prepare custody
assertion; they do not yet demonstrate execution of the nonfinite preparation
failure suffix. The other failures expose eager release, absent physical bounds,
capacity bypass, expired publication/rate effects and nonfinite native work.
No compilation or fixture failure is counted as an assertion RED.

Working controls pin both ticker response orders with blank bid/ask fallback,
book-only exclusion, valid-row catalogue retention and exact native diagnostics,
broken streams, current429 pause, sibling delivery before/after seed reset,
genuine256-peer reuse, and fixture-entry equivalence to actual Engine.start.
Nonfinite controls invoke the same native start under an engine-issued callback
without enabling the engine's unrelated startup watchdog; a finite working
control compares every request's method/URL/header/body/route/timeout to actual
Engine.start. Returned/preparation failure controls retain an unrelated genuine
finite peer and require complete credit recovery after exact sibling cleanup.
There is no fabricated Context, permit, issuer, next ID or preparation token.

The existing public15 and startup_resilience5 controls pass unchanged. The new
target passes scoped Clippy with -D warnings and rustfmt. A first fixture import
used Book at the wrong public path; it was corrected to book::Book before the
qualified evidence. The missing historical Cargo binary path also failed before
compilation and is excluded. Logs remain under /tmp/codex-weex-public-seed-*,
not in the repository. Runtime, prior fixtures, Cargo and inventories remain
byte-exact to c5d7. Source awaits bounded original review; paired WEEX+combined
measurement remains a gate at the first source commit.

# GRVT unsigned FetchOrderBook command custody

Plan-first stack: actual `1a341fbb31b` through reviewed-in-progress candle draft
#2062 at `5e7e0c528ad`. This slice is sequenced after #2062 because both own narrow
GRVT lib.rs branches. The current checkpoint changes docs only. No production,
existing fixture, Cargo or manifest changes are authorized by this checkpoint.

## Owned sites and source boundary

The complete native unsigned FetchOrderBook BookQuery path has six occurrences:

| Path / native region | Occurrences |
| --- | ---: |
| lib.rs: raw self.request for /lite/v1/book | 1 |
| lib.rs: unknown-market empty successful book | 1 |
| lib.rs: BookQuery fail_pending terminal | 1 |
| depth_query.rs: native refusal, decoded success, invalid snapshot failure | 3 |

After #2062, lib is 11 and depth_query is 3, GRVT family 41. This slice targets
lib 11 to 8 and depth_query 3 to 0, family 41 to 35. Only those two exact manifest
keys may shrink. Other shared request calls, account, identity, history, private
trading, signing, streaming books and subscriptions remain outside this adoption.
Auditor confirms no current combo-5b GRVT source path; key selection is unchanged.

## Native contract

FetchOrderBook resolves the Store market first. An unknown symbol succeeds with
{"bids":[],"asks":[]} and no I/O; requested limit is ignored in both cases.
For a known symbol, POST the MDG mainnet/testnet /lite/v1/book endpoint unsigned,
with exactly Content-Type: application/json and ordered body {i: native id,d:50}.
No subscription, cached book, credential, Session or signer is required. No Store
book may be published or replaced by this independent command result.

Keep depth_query.rs's typed native response codec: object boundaries, root and
nested code/message/error precedence, malformed-code refusal, missing result as
empty book, optional bids/asks, order and duplicate levels, positive prices,
nonnegative amounts and finite cumulative totals. No reordering, level merge,
sequence coercion or extra validation. Preserve rate-limit retry-after clamping
and native venue_code/venue_message, HTTP/status classification and strings.
Native failures carry NotSent even for this Read; preserve that compatibility
metadata on native refusals and invalid snapshots. New shared admission refusals
use the normal Read outcome none, and preparation refusal keeps shared NotSent.

Native on_request_failed performs initial plus five immediate transport attempts
without backoff. Native BookQuery, like Candles, has timeout_ms=None. The proposed
20 s admission/route/Prepare/Bind/physical attempt bound and derived 120 s ceiling
are NEW caller contracts approved by the overseer, not inferred host timeouts.
Document them in CALLERS before source/freeze. Original caller receipt wins;
late attempts shorten or refuse below 1 ms remaining, and retries never renew it.

## Existing shared API

Begin genuine command ownership with begin_preflight_with_command_wait using
PreflightPurpose::ReadOnly and no credentials, before Store lookup or completion.
A rejected admission owns the single terminal. Revoked expired entry uses the
existing reject_expired_command(Read) path, without native fallback or renewal.
Unknown-market empty success uses done_preflight and retains the original receipt;
it neither creates a fake HTTP call nor borrows numeric command authority.

A known symbol admits one Step / one CallBound {timeout_ms:20000, attempts:6,
retry_window:FreshAttempt}, PlanPolicy {Read, no credential, ReadFailure}. Issue
one exact Slot/Context, then request_managed_with_headers, static public headers,
no stream selector. Job::BookQuery owns Plan/Slot/leg/body rather than CommandId.
Validate_plan_delivery before status, parsing, diagnostics or result consumption.
Make the native decoder return its result or VenueError without numeric terminal
side effects. DATA success calls done_read_plan_after_data exactly once; do not
finish_plan_call first. Native failure seals the Plan with its existing metadata.

Only typed on_http_failed Network or engine physical Timeout may select
plan_retry_context(ReadFailure); no error-text inference or untyped retry.
Integrity/Internal and status/protocol/codec failures seal strictly. Preparation
and provider/bind denial remain strict through on_http_preparation_failed. The
sixth transport failure is terminal with the native Network/NotSent refusal and
no seventh request. Revoked or stale delivery cannot fail a live replacement.
plan_retry_context Err(Revoked) returns silently; every other refusal seals the
Plan strictly. No new engine API, history authority, retry grant, timer or queue
is needed.

## Composition and preservation

Preserve every candle source/control from #2062, including its typed integrity
fix. Add BookQuery-only typed callback arms and delegate all other jobs exactly
as before. Route BookQuery before #1905's login-only Pending.context take blocks,
keep that field None for BookQuery, and own Plan custody inside Job::BookQuery.
Whichever integrates #1905 and BookQuery second runs the real login preparation-
failure control. Login is not a prerequisite for this public unsigned slice.

The candle preparation-delegation guard currently uses a synthetic BookQuery
pending as its non-Candles oracle. When BookQuery is migrated, move the guard to
a still-legacy native pending family with a real default-chain distinction;
retain the differential/default/direct-fail negative proof and real managed
Prepare/refusal/Cancel/duplicate fences. Do not weaken that test to fit the new
BookQuery behavior. Coordinate the exact fixture choice with 55ao7t before source.

## Original controls before production

Add a dedicated request_context_book_query target through genuine Engine entry,
caller receipts and Prepare/Bind/Claim. Plan and original review precede source.
Pin both deployment wires; unknown-market empty success; ignored limit; native
code/message/headers and malformed/empty/duplicate/cumulative-level semantics.
Use 256 genuine retained Command Read peer roots: local empty and remote work
must refuse before native effects; retire exactly one and validate all 255 after
recovery. Add separate external-event NaN local/remote and duplicate controls.

Real custody REDs cover eager raw transport, unclaimed DATA, strict preparation
(including Timeout while the caller lives), no callback needed at caller expiry,
late DATA, fresh physical retry window and shortened caller cap. Pin six actual
released failures/no seventh, old reply/failure inertness, and non-UTF8/invalid
full-response integrity strictness beside Network/physical-Timeout positives.
Use actual ingress; compile/setup failures and missing APIs are not RED proof.
Disclose every first failing assertion and gated suffix; no execution claims for
unreached cleanup, recovery, cap or timeout assertions. Preserve existing native
book assertions while adapting only fixture dispatch to prepared_actions.

Scoped cargo only, CARGO_BUILD_JOBS=4, one own build at a time. CI covers full
workspace/features/WASM/size/replay; source review and green CI precede READY.


## API review pins before originals

Fe7 cleared plan 82570c4b6ff. Originals must distinguish HTTP 408/504 DATA
(refusal kind Timeout, one native NotSent terminal, no retry) from the genuine
released physical-Timeout retry. A 429 with Retry-After must keep native clamp,
venue_code/message and NotSent, one terminal and no new Prepare or auto-defer.
Network failures still receive the six declared immediate attempts.

CALLERS must explicitly disclose the local ordering change: shared admission
and capacity precede unknown-market empty success, so a full pool now refuses
where the native path succeeded without I/O. The 256 retained peer-root local
empty control pins that new contract and exact recovery, alongside the remote
capacity control. No projection or synthetic request can bypass it.


## Compiling original-control checkpoint

The dedicated target runs 24 controls against unchanged BookQuery production
on stack 5e7e0c528ad: 16 intended assertion REDs / 8 GREEN / zero ignored.
No runtime, Cargo, map or existing fixture is changed by this checkpoint.

RED first failures: main/testnet and unclaimed/preparation/physical-timeout/short
cap/stale-reply/stale-failure cases assert managed Prepare and instead see raw
HTTP. Remote capacity and remote NaN see raw HTTP; local capacity and local NaN
see the native empty success. Late claimed DATA succeeds beyond the caller cap;
the original caller timer is absent. Non-UTF8 actual ingress selects a raw retry
where shared typed integrity failure must be strict. No missing-method errors
or compile failures count as RED evidence.

The original recovery and strict-preparation cleanup suffixes remain unexecuted
after their first RED: no claim yet for exact Cancel, retry-hint preservation,
255-peer survival, fresh/short windows, final cap expiry or stale replacement
settlement. They are required source-stage gates. Physical 20 s expiry and the
120 s derived ceiling are NEW contracts, not a native timer-bug claim.

Eight GREEN controls pin both exact deployment wires/level order/duplicates/
cumulative totals/no Store book publication, empty/alias/ignored-limit behavior,
strict malformed/overflow rows, six immediate attempts and no seventh, strict
HTTP 408/504 Timeout DATA, 429/1006 RateLimited metadata and clamped hint with
no Prepare or deferral timer, duplicate ownership and zero-wait refusal. The
rate control retains venue_code/message and native Read NotSent.

An initial fixture compile used SetTimer.at instead of at_ms; corrected before
this receipt. The NaN-local check now explicitly asserts refusal before reading
its error metadata, so its RED is the contract assertion, not unwrap_err on a
native success. Neither setup experiment is included in the reported counts.

Scoped strict Clippy on the new target, rustfmt and diff checks pass. Cargo used
four jobs serially; no workspace, all-feature or WASM build was run locally.

Fe7 original review acb6 is CLEAR for production. Before source, the two timeout
controls now additionally assert the actual Bind/Claim packet timeout (20 s or
remaining caller cap). Re-executed strengthened originals remain 16 assertion
RED / 8 GREEN / zero ignored on unchanged production; those suffixes remain gated.


## Production checkpoint

BookQuery now owns Plan/Slot/leg/body, admits before lookup, and uses the cleared
single-call Read Plan. Unknown-market empty completes through done_preflight;
exact DATA delivery is validated before the native pure response decoder. The
decoder returns its existing result or VenueError; its status, native-code,
field precedence, level order, cumulative arithmetic and metadata are unchanged.
Typed Network or physical Timeout alone selects retry, Revoked is silent, all
other retry refusals seal strictly. Status Timeout and RateLimited remain DATA
terminals. Preparation and untyped failures never authorize a retry.

The strengthened 24 original controls all pass after cutover, and the complete
GRVT crate run passes 260 tests, zero ignored. Scoped all-target strict Clippy
and formatting pass after collapsing one nested if as requested by Clippy.
The first full run exposed raw-only public/trading fixture drains; they now
drive prepared_actions, preserving every original native/financial assertion.
The book fixture excludes only shared SetTimer/ClearTimer bookkeeping when
retaining its one-transport assertion; rate-limit no-timer behavior is pinned
independently by the original target's complete action batch.

As agreed with 55, the preparation-delegation control now uses real credential-
free Query::PublicTrades state. It boots the native catalog, captures the native
public pending through a released failure callback and queues that pending at
a real managed Prepare. Venue/default chain results still match; a direct
fail_pending mutant differs, exact Cancel and duplicate refusal fences remain.
No Identity, private account or login fixture is introduced. An initial wrapper
omitted catalog item forwarding, leaving no selected market; that fixture setup
was corrected before the passing receipt, with no production history change.

Scanner-exact scope is lib 11 to 8 and depth_query 3 to 0, GRVT 41 to 35. Only
those two map keys change; foreign manifest bytes/order and all candle source
blobs remain exact to the parent. No engine, host, Cargo, signer, private reducer,
queue or Session source is changed. Shared callback edits are BookQuery-only.

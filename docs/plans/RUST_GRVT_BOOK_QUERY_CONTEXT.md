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
No new engine API, history authority, retry grant, timer or queue is needed.

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

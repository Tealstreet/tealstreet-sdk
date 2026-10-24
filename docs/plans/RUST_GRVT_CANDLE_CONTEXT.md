# GRVT unsigned FetchCandles Read Plan

Plan-first base: actual `10d01de52795ffdc28e75e2c1fc734495b01217b`.
This checkpoint is plan plus compiling original controls only. Production,
existing fixtures, Cargo, checkers and maps remain exact to that base.

## Slice and exact inventory

Complete the existing unsigned FetchCandles command, not the generic request
funnel or other public/private jobs. Seven numeric terminals in lib.rs are owned:

| Native region | Occurrences |
| --- | ---: |
| Entry: unsupported interval, unknown market, invalid bounds | 3 |
| Response: malformed rows, successful serialization, serialization failure | 3 |
| fail_pending: GRVT candle read failed | 1 |

Intended lib19->12 and family49->42. The shared raw request/request_streamed_body
fingerprints remain for other jobs. No rename-only prune, whole-funnel claim or
claim that seven terminals mean seven HTTP call sites. Repeated hashes must keep
the exact foreign occurrence counts. Only the GRVT lib manifest key may shrink.

Auditor8x confirmed current combo5b train/combo-5b-3573 at989603e75a4 has no GRVT
path, including lib.rs and Cargo.toml. A future grvt engine-dependency http-plans
feature opt-in is clear; do not edit workspace Cargo.toml or Cargo.lock without a
fresh boundary check. The current checkpoint adds no shared plan-index row.

## Native wire and semantics

Keep unsigned POST /lite/v1/kline on market-data.grvt.io or
market-data.testnet.grvt.io, no credential, static ordered Content-Type JSON,
public routing and no Session/Bearer/signing. The body keeps ordered fields
{i,t,i1,st?,et?,l}: native catalog instrument id; t=TRADE; existing interval map;
optional millisecond bounds formatted after multiplying by1e6 using the existing
native formatter; limit default500 clamped1..1000. Preserve omission and all
current numeric behavior; add no range or overflow validation.

Validation remains interval first, then Store market, then finite/nonnegative
from/to. Preserve exact strings and unclassified native Read failure metadata.
Retain stream ArrayAt::Envelope(r,result), wire::Feed::candle validation, malformed
row flag, exact ns->seconds conversion, sorted ascending timestamps, duplicates,
empty-array result and missing-array/invalid-envelope refusal. Rows from a failed
attempt are discarded before retry; no prefix success, cache or Store write.

Native on_request_failed increments attempts while attempts<5 and immediately
calls send_request: one initial plus five transport retries, exactly six physical
attempts with no backoff, pause or per-attempt waiting. Native HTTP/status/code,
missing-array or malformed-row failures do not retry. Code1000 for Candles is
explicitly excluded from account-refresh/auth retry. No native auth grant.

## Existing API fit and original deadline

Fe7 reviewed the proposed shape against actual10d01: no engine API gap.
At genuine command entry use begin_preflight_with_command_wait(ReadOnly,None),
retaining the exact caller receipt. The three synchronous validation refusals
use fail_preflight before Plan admission. A shared admission refusal already owns
its terminal; do not publish the native error a second time. Revoked expired
entry uses the established reject_expired_command(Read) handling, without renewal.

For valid work admit one StepBound with one CallBound: timeout_ms20000,
attempts6, RetryWindow::FreshAttempt; PlanPolicy Read/no credential/ReadFailure.
The logical call span is6*20s and the Plan deadline is min(native span, original
caller receipt), not a fresh120s/default surrogate. Each released attempt receives
its own native20s physical window under that same cap. A late retry may get a
shorter window, or refuse with less than1ms remaining. Pin that behavior so a
caller cap is never mistaken for a lost native retry or renewed at dequeue.

Queue using request_managed_with_headers plus stream_response on the actual
returned request id. At every streamed item and final DATA callback call
validate_plan_delivery(plan,leg) BEFORE codec, rows, native status, diagnostic or
terminal. Success uses done_read_plan_after_data(plan,slot,leg,result) in the
exact DATA callback; it settles the call itself, so never finish_plan_call first.
There is only one logical call: no early suffix, accepted-prefix staging or
#2011 history authority. No Context cast, engine change, Session or readiness wait.

Only an engine released transport/physical-timeout callback can select
plan_retry_context(plan,slot,leg,ReadFailure), then queue that selected attempt in
the same custody. Existing failure tail sees Ready and retains the Plan. Attempts
are engine-counted; the sixth released failure exhausts the budget and the next
selection returns RetryForbidden. Fail with the native generic candle error;
there is no seventh Prepare, and duplicate failure cannot spend another attempt.
Do not infer release evidence from error text. Preparation/provider/bind failures
are strict and retain typed shared error fields, with no retry or empty success.
Native HTTP/status/protocol/codec failures remain strict DATA refusals.

## Narrow future source and composition boundary

Future source: a dedicated candle request module, lib.rs Candles Job payload,
FetchCandles entry, Candles-only send/fail_pending/item/response/stream-failure/
typed transport/preparation branches, and Cargo http-plans feature. A Plan/Slot/
leg live in Job::Candles, with per-attempt rows and malformed flag. No caller
CommandId fallback; no venue timer, queue, generation, budget or reselection.
Keep generic request/private send/retry, every other Job, account/identity/history,
BookQuery, subscriptions, ticker/catalog, financial reducers and signing exact.

55ao7t cleared ownership versus #1905 at1a40cb7d663: Pending.context means ONLY
the managed login Session write. Candles must keep it None and carry the Plan
inside Job. Route Candles response/failure before the login context.take hooks.
New typed callbacks must delegate every non-Candles pending to the engine's old
default (on_sign_error for preparation; on_request_failed for transport), never
fail_pending. Preserve login_failed/session retirement. A post-#1905 login
preparation-failure composition control is required before final readiness; use
prepared_actions in native fixtures so both pre/post-login custody works.

## Original controls before source

Use a new target and genuine Engine command receipts, retained peer roots and
Prepare/Bind/Claim. Separate mainnet/testnet eager-HTTP REDs, shared entry capacity
before each local validation, remote capacity, invalid clock, unclaimed DATA,
caller expiry without another venue callback, typed preparation refusal, six
released transport attempts and no seventh, physical-timeout fresh windows,
short remaining caller cap, stale response and duplicate failure. Working native
controls pin exact wire, field order/limits/bounds, sorted valid/empty rows,
malformed/protocol/status strictness, and initial+five immediate retries. No key
material, remote signing decision or false native Auth evidence is needed.

Compile/setup failures and missing-method errors are not REDs. Record first
failing assertions and gated suffixes. No production or existing assertion edits
until plan/original review. Use CARGO_BUILD_JOBS=4 and one scoped build at a time;
CI owns full workspace/features/WASM/performance. Original plan/API clearances
are not production source or actual-prerequisite receipts.


## Original execution checkpoint

Plan3b69de0eda3 precedes the dedicated request_context_candles target. Fe7's
existing-API fit is CLEAR, auditor8x confirms the current5b file boundary, and55
confirms the narrow public Candles ownership and login composition constraints.
The retry timing audit is explicit: native send_request is called immediately,
with no retry backoff. Six released failures terminate, and a duplicate seventh
failure notification produces neither another physical attempt nor a terminal.

The original target compiles and executes23 controls on unchanged actual10d01:
15 intended assertion REDs,8 GREEN, zero ignored. Separate mainnet/testnet REDs
show eager raw HTTP before Prepare. Three local capacity REDs retain the native
unclassified error instead of shared Capacity; remote capacity and invalid-clock
cases emit raw HTTP or native validation. Unclaimed DATA, real physical-timeout
fresh-window, typed preparation, short retry-cap and stale-reply controls fail at
the disclosed first Prepare boundary; their deeper suffixes are unexecuted gates.
Claimed late DATA still succeeds beyond the caller cap, and no caller root timer
exists on the original path. No missing API or compilation failure is RED proof.

Eight working controls pin both deployment wires and native rows/empty results,
exact validation precedence/messages, malformed/status/native-code strictness,
six immediate attempts with no seventh, duplicate original-peer retention,
invalid caller-wait refusal, limit clamping/bound formatting and omission, and
partial streamed-row discard before a transport retry. The frozen native interval
message is GRVT does not support 1s candles. The row-prefix control is native
reset behavior, not accepted-prefix history authority or a soft-success claim.

Shared-capacity recovery validates all255 retained peers after the first peer
retires and the retried command admits; that suffix remains gated by the initial
Capacity RED. Preparation-failure suffixes inspect the complete batch for no
outbound and exactly one matching unsigned Cancel. Cancellation is cleanup, not
an outbound transport. No native metadata, retry or signature rule is inferred
from that gated suffix.

The initial fixture expected a different interval-error string; corrected against
Interval::unsupported before the final receipt. The added streamed-prefix control
initially used HttpChunk.chunk instead of the existing data field; its compile
failure is excluded from all RED counts. Production, existing tests, Cargo and
maps remain exact to the assigned base; no shared file is changed.

Scoped strict Clippy on the new target and scoped rustfmt/diff checks pass.
Local cargo used four jobs, sequentially; no workspace or all-feature sweep.

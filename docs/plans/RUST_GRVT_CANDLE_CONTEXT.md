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
attempts with no backoff, pause or per-attempt waiting. Native Candles uses
request_streamed_body with timeout_ms=None; only Builders sets an explicit10s
request timeout. That source does not prove a native20s attempt window. Native HTTP/status/code,
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
The20s admission-to-expiry attempt policy and derived6*20s logical ceiling are
NEW caller contracts, including stuck route/Prepare/Bind time before release.
The Plan deadline is min(declared120s ceiling, original caller receipt), not a
fresh120s/default surrogate. Each selected attempt receives its own shared20s
physical cap under that same original bound. These bounds are not inferred from
the native timeout_ms=None or a default host timeout; no native20s claim is made. A late retry may get a
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
preparation-failure composition control is required when #1905 is composed
before this slice freezes; use prepared_actions in native fixtures so both
pre/post-login custody works. If #1905 lands later, its integrator owns that
same composition control. The unsigned public slice needs no login capability
or actual #1905 prerequisite; current non-Candles delegation must remain exact.

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
the disclosed first Prepare boundary; their deeper suffixes are unexecuted gates. The physical
20s timer and120s ceiling controls assert NEW caller contracts, not an original
native physical-timeout bug.
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


## Required timeout-contract correction before source

9cz's native review identified that the raw candle request sets timeout_ms=None.
The proposed20s admission/route/Prepare/Bind/physical policy and declared120s
logical ceiling are therefore explicitly NEW bounded caller contracts. Fe7
accepted that correction with no engine API change. Production must add a CALLERS
entry disclosing both bounds, preparation expiry and original receipt precedence;
that disclosure is a source/freeze gate. No host timeout or native physical window
is guessed. The native six immediate transport attempts remain exact, and the
physical-timeout test is correctly labelled new-contract evidence.

Fe7's no-seventh-attempt P3 is already pinned on the sixth failure's own batch:
strict_error begins with no_io, whose managed arm forbids Prepare and Bind, before
the duplicate failure control. Fe7 accepted that citation as closure. His optional
cap-minus0.5ms control remains optional source-stage strengthening; it is not
claimed by the current shortened-window control. The login preparation-failure
composition gate remains mandatory for whichever integrates #1905 and this
slice second; public candles do not depend on a login capability.

The corrected plan/diagnostic target re-executes the same23 controls:15 intended
assertion REDs/8 GREEN/0ignored. Assertion conditions and runtime APIs are unchanged.


## Login-independent preparation delegation control

Before production, a real non-Candles managed Prepare is refused through GRVT
and compared with the engine's inherited on_sign_error default chain. Both keep
the same native BookQuery continuation, exact wire and no terminal. A deliberately
direct fail_pending disposition produces no continuation and differs, proving
the control detects that callback regression. Exactly one unsigned Cancel and
duplicate-refusal inertness are pinned. This preserves an existing legacy path;
it grants no managed fallback to Candles. The control passes with four build jobs.
Fixture compilation omissions and initial assumptions about command terminals
were corrected against both real paths before this receipt; neither is RED proof.
55 accepted the conditional login-specific gate: the second integrator runs it.


## Production checkpoint

After plan/original review CLEAR and the native timeout correction closure,
Candles now carries exact Plan/Slot/Context custody in Job::Candles, with one
ReadFailure declaration (one call with six FreshAttempt attempts of 20 s) under the retained
caller receipt. Each DATA item/final/stream error validates exact delivery before
codec or terminal. Only released failure dispatch selects a retry; preparation
failures seal strictly with shared typed fields. No Session, signing or key
selection is involved. Non-Candles callback delegation is unchanged and tested.

The scanner confirms eight actual removals: the seven numeric terminals plus
the physical raw self.request candle call that the initial estimate omitted.
Thus lib 19 to 11 and family 49 to 41, with no new fingerprint or other manifest
key change. This is source-proven elimination of the owned candle funnel, not
renaming or a claim about any other Job. Original assertion bodies are unchanged.

All 23 original controls and six library controls pass after adoption. The
existing product fixture initially could not find kline because it drained only
raw HTTP; it now uses prepared_actions to drive genuine release. The existing
public candle fixture does likewise, preserving every original assertion.

The complete changed-crate test run passes 232 controls, zero ignored, after
fixture adaptation. The first ratchet run passed all 24 checker controls but
reported foreign Backpack/BingX/Crypto.com/WEEX inventory drift against the
new landing. Integrate actual before publishing; this is not a candle failure
or permission to overwrite foreign manifests. Source review and CI follow.


## Typed integrity failure correction

Uuf's source review found a real gap in the inherited on_stream_malformed /
on_http_failed / on_request_failed chain: dropping Internal caused a malformed
stream to select ReadFailure. Two genuine ingress controls (non-UTF8 and an
unfinished envelope) independently reproduced one new Prepare and zero terminals
before the fix. Both are appended after the unchanged original 23-test prefix.

Candles now keeps typed on_http_failed: only engine Network or physical Timeout
may select retry; integrity and every other typed failure seal strictly. The
untyped Candles failure callback is strict too, so strings confer no retry
evidence. Every other Job delegates to exactly the old on_request_failed default.
Preparation failures remain a separate strict typed callback. A preparation-
Timeout control proves it cannot enter the released transport retry path.

The two integrity controls now pin one Internal failure / Read outcome none,
no new Prepare/Bind/raw HTTP and inert late replies. Existing network and physical
timeout positives remain. A late old-attempt failure control preserves the live
replacement; the retry helper returns silently on Revoked per fe7's P3. No
checker, map, wire, native status, credential or Session contract changes here.

After the correction, the changed-crate native run passes 236 controls, zero
ignored. The original 23-test byte prefix is exact; four source-stage controls
are appended. No existing financial/native assertion was changed.

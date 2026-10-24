# BingX history paging on the original caller Read root

Status: production checkpoint under native/bridge review; no landing.
Source pin: actual `b3688e4c04d`. Owner: codex-cxxyqr, transferred from
codex-uufub4 by the overseer. Native reviewer: claude-4yjlw0; bridge/API reviewer:
codex-uufub4. This is a Context-root consumer, independent of #2011 Read Plans.

## Scope and existing authority

Convert the remote paths in `sdk-rs/venues/bingx/src/history.rs`: `next`,
streamed item/response admission, delay continuation and terminal ownership.
The narrow `Pending::History` routing in `lib.rs` carries exact delivery,
typed failures, the signature resend and the no-Cx root-retirement hook.
The four already-managed local-query branches remain unchanged, including
credential/unsupported/symbol/range precedence. Account cycles, metadata
groups (#1747), trading, identity, candles and shared signing funnels are held.

Actual #1848 supplies ReadCompletion, ReadHistoryRows and ReadRangeHistory.
Actual #1945/#1949/#1976 supply original receipt custody, typed missing-receipt
refusal and strict engine root expiry. Actual #1983 `a2533f5761f` supplies
SelectedRead::command_root, immutable ReadRoot and move-only ReadLeg. No shared
engine API, Background fallback, Plan-as-Read cast or numeric completion remains
in the converted history path. No Plan is needed merely for paging.

## Native geometry, bytes and failure contract

All remote history endpoints are GET. Private selection is Secondary when
configured, otherwise Primary, from the same immutable credential snapshot.
ReadRole::Read has scope None: native history adds no account-auth evidence.
The existing credentials-absent local `[]` rule stays before remote admission;
public trades remain unsigned. Keep Params sorting/rendering, query names,
X-SOURCE-KEY/content-type headers and the existing dispatch::Http recipe.

| Family | Native path and geometry |
| --- | --- |
| Fills | `/openApi/swap/v2/trade/fillHistory`; missing symbol returns local `[]`; optional orderId query filter; default seven-day range; `startTs`, `endTs`, `pageSize`, pageIndex starts at 1; limit default 100, clamp 1..1000. Inclusive windows at most seven days, next begins at previous end+1 ms. A full mapped page increments pageIndex, max 1000 per window; no new identity or an out-of-window row rejects. Short pages advance the window or finish. |
| Orders | `/openApi/swap/v2/trade/allOrders`; optional symbol; `startTime`, `endTime`, `limit`, default 500, clamp 1..1000. Paired unequal bounds use the existing RangeWalk with seven-day chunks and a 50-page-per-chunk cap. No bounds use one response; equal bounds reject a saturated page. |
| Income | `/openApi/swap/v2/user/income`; optional symbol/incomeType; default 100, clamp 1..1000. Same RangeWalk and equal-bound saturation rules as orders. Income identity remains tranId/type/value/time. |
| Public trades | `/openApi/swap/v2/quote/trades`; one unsigned response, default 500, clamp 1..1000, native symbol and original response order. |

One-sided private bounds are filled natively: missing end becomes now, missing
start becomes end minus seven days. Unequal resulting order/income bounds use
RangeWalk. Truncation walks remaining chunks and rejects only at Finished;
it must not stop early. Retain its split/progress rules and ChunkDone loop, and
the 200 ms timer after every fill/range page. Do not invent a new total page cap,
whole-query geometry bound, pause or transport retry. Preserve native row
conversion, fill seconds versus order/income milliseconds, fill IDs/dedup,
sorting newest-first except public trades, and all malformed/envelope errors.

IMPORTANT native distinction from WEEX: BingX rejects the entire query on
every page failure, even after earlier rows were accepted. `fail_read` does not
return partial rows, and `failed_page_reports_error_without_a_partial_success`
already pins it. Use the bridge's strict reject, never accepted-prefix recovery
or an INCOMPLETE marker. Truncated RangeWalk/equal-bound saturation also reject.
Provider/preparation refusal remains typed NotSent and creates no auth inference.

Native signed requests can re-sign ONCE per page after code 100001 or the
signature-verification text, including the stream-error route. Preserve that
predicate, diagnostic and one-resend limit. Validate the rejected delivery,
discard only its unaccepted staging, retire its leg, then derive a new Never
child from the SAME selected root and same page parameters. Do not remint the
root, reselect credentials or add transport retry. Both physical attempts keep
the native 20 s default, capped by the original root. A second rejection fails.
Native resend retains Request rows/error (only candles clear them today).
Normal signature rejection has no rows; the conversion's explicit discard
prevents rejected-attempt staging crossing into a new admitted attempt. State
this custody change in CALLERS, without claiming native already discarded it.

## Custody and direct cutover

After native local validation, a genuine remote query captures its caller wait
exactly once. Expired uses reject_expired_command(Read); missing/failed receipt
uses reject_unstamped_command(Read), with no fresh default in the venue.
Signed reads select then call command_root; unsigned reads use the existing
admit_caller_read_root(None, None). Root deadline is immutable through all
200 ms delays, seven-day windows, RangeWalk chunks and signature resends.

Retain root/selected snapshot and ReadCompletion in native state. Use
ReadHistoryRows for every native pager, with FillPages and the existing
RangeWalk retained under validated row admission. ReadRangeHistory records
coverage for partial success but exposes no strict truncation verdict; using it
here would change BingX policy. The native RangeWalk is the sole geometry and
coverage state, with no copied policy, second row list or terminal owner. Staging/filtering/identity/page acceptance
must borrow the exact root/physical Context/Attempt admission. Validate before
row codecs, count/progress mutation, envelope side effects and publication.

Mint each 20 s Never child only at eligible native send/dequeue. A managed
Prepare/Bind/Claim gates release. Carry the ReadLeg and exact page intent in
Pending, including the resent flag; signed queueing uses the retained Selection.
Public request_managed_with_headers preserves native static headers and stream
selector. Keep the fill EnvelopeFieldOrNestedAny and other data selectors.
Finish the previous physical leg before continuation or resend; successful
terminal publication precedes physical retirement. Failure/sign/stale cleanup
retires first. Mint/queue refusal strictly rejects the exact live root with the
canonical typed Read error, releasing native state/timer; only proven-revoked
authority drops inertly, also removing native state. Returned queue Pending is
reclaimed, not orphaned.

Every terminal goes through the original bridge owner exactly once. New typed
physical Network/Timeout failures retain their shared Read projection (outcome
None) rather than demoting to native text; existing native message text and
provider NotSent values stay intact. No finalizer runs on failure or expiry.
read_root_retired removes delay/native state by exact Owner including generation;
it has no Cx and cannot finish numerically. Late DATA/Bind/timer/failure is inert,
including after command-ID reuse, without suppressing an unrelated live query.

## Original assertions before runtime

The new integration target drives actual Engine<Bingx>, real handle_command
receipts and real provider recipes. A drain counts legacy Http and follows
managed Prepare/Bind/Claim so native URL/key positives run before the final
custody assertion. Each independent case is a separate test.

1. Private fills, orders and income each preserve native GET query/header/key
   positives, then require zero legacy release and one Prepare/Claim (RED).
2. Public trades preserve unsigned native headers/query, then require the same
   managed release (RED).
3. Root expiry while a fill continuation is parked yields one typed Timeout,
   no rows/marker; the stale delay cannot issue page 2 (RED).
4. Public and signed in-flight DATA at the exact original receipt deadline
   cannot complete successfully; strict Timeout wins (independent REDs).
5. A raised caller wait permits an in-bound public result (GREEN control).
6. Accept a full fill page, then fail the next page: one strict native failure,
   no CommandDone or INCOMPLETE (GREEN control, do not count as RED).
7. Native one-time signature re-sign preserves pageIndex and selected read key,
   and a second rejection fails without a third request (GREEN control).

Readiness controls after cutover additionally pin immutable roots across native
windows/chunks and resend, override above 120 s, fractional predeadline versus
equality, provider refusal after accepted output, physical leg Timeout strictness,
root/child capacity and atomic queue refusal, exact-generation reuse/live peers,
and no-Cx cleanup. Original body assertions remain unchanged; fixture adapters
may only adopt managed transport mechanically. Missing-method/compile failures
never count as original RED evidence.

## Inventory, callers and gates

At the source pin, history.rs has four inventoried terminal occurrences/keys.
The later production conversion must remove them genuinely and regenerate only
the exact removal-only source inventory, preserving incoming ordering. `issue`
is a wrapper, so do not claim that its semantic conversion prunes an extra
lexical request site. STAGING and checker policy remain untouched. Other
history-independent lib.rs signing/request sites remain for their owners.

The source commit's CALLERS block must state managed host preparation, original
caller-wait expiry, strict failure/no-partial behavior, typed Read projection and
the unchanged one-time signature resend. SDK caller timeout still abandons only
its listener; engine root expiry is separate and authoritative. No NX execution
receipt is inferred from the host model tests adopted into #1976.

Plan and original RED phase changes only this plan, README index and the new
integration test. Production waits native/bridge review and explicit conversion.
Later gates: scoped history/local-query suites, strict all-target Clippy/fmt,
current-tip bans/ratchets, preservation/source review, fresh exact-head CI and
BingX replay CPU/WASM size. No merge or readiness is claimed by this plan.

## Original checkpoint and review receipt

Plan `67782b4cbcb` precedes the integration target. Native review by
claude-4yjlw0 and bridge/API direction review by codex-uufub4 are CLEAR; native
Low notes on local missing-symbol results, orderId, one-sided bounds, late
truncation and retained rejected-attempt staging are incorporated above.

Executed on unchanged production with:
`cargo test -p safe-cex-venue-bingx --test history_context`.
Result: 7 assertion REDs, 3 GREEN, 0 ignored, no compile or fixture failures.
The four release cases pass their URL/query/key/header positives, then observe
`(legacy, Prepare, Claim) == (1,0,0)` instead of `(0,1,1)`.
Parked expiry emits no terminal; signed and public exact-deadline DATA each
return successful `[]` instead of strict Timeout. These are separate tests.
The three independent GREEN controls preserve native strict later-page failure,
one-time same-page/read-key re-sign followed by strict second rejection, and
an in-bound public result under a raised caller wait. The latter is not proof
of a walk continuing beyond 120 seconds. All remaining production/readiness
controls above remain gates, not executed adoption evidence.
The unchanged native history target passes 20/20 and local-history custody
passes 5/5. The new target passes strict all-feature Clippy and workspace fmt.
Only this plan/index and the new integration target differ from the source pin;
all production, shared engine, checker and inventory blobs remain byte-identical.

Native original-evidence review at `dc299373024` is independently CLEAR from
claude-4yjlw0 (archive run reproduces 7 RED / 3 GREEN). Its three readiness
notes remain required: explicitly exercise Primary fallback without a read key;
pin zero legacy and one Prepare/Claim on both page-2 and signature resend after
conversion; use a separate refusal-aware host driver for deferred claim-refusal
controls, since the original evidence drain intentionally asserts Released.
Do not change the original expected values to disguise these missing controls.

Implementation refinement before source: retain native RangeWalk plus one
ReadHistoryRows owner because BingX must reject truncation only at Finished,
whereas ReadRangeHistory owns partial-success coverage. A RowsAdmission borrow
guards native range page/progress and accepted output together. No new shared
getter, callback or coverage policy is introduced.

## Production checkpoint on actual 90ed853c6cd

The actual landing is plain-merged at `f3767846984` before runtime edits. Remote
reads now retain one SelectedRead/public original caller root and one
ReadHistoryRows authority. Request owns its physical ReadLeg/Context and native
page intent. Delivery gates precede codecs, native pager progress and guarded
row acceptance; root validation precedes dequeue. Strict reject replaces all
four numeric terminals. Native one-time signature resend gets a fresh child
from the same root/selection and drops rejected-attempt staging. The no-Cx hook
removes exact-owner state. No shared engine or signing-recipe code changes.

All ten original custody assertions pass unchanged. Thirteen added readiness
controls pass, including Primary fallback, managed page-2/resend counters,
default/raised receipt walks through page 7, a >120 s completion, fractional
liveness, physical timer and late-DATA Timeout, reused ID plus live peer,
provider/route refusal after accepted output, delayed Bind refusal, and genuine
child-capacity saturation plus peer release/fresh admission. The capacity test
compares the exact error with the same-harness genuine admission control. This
is a child-mint capacity test, not a separately injected synchronous queue error.

The native history fixture now drains managed preparation and tracks cleared
timers/deadlines, firing the earliest live timer. The existing provider-failure
fixture similarly distinguishes the native 200 ms delay from the root timer
and keeps later events at that clock. Financial/message expectations remain;
all twenty original history test bodies and all ten original custody bodies
are verbatim. Financial codec functions and the local-result helper are
byte-identical to actual landing.

BingX all-targets: 258 pass, 0 fail, 2 pre-existing ignored paired-metadata
original REDs (outside this slice, still waiting for #1747/#1919 in this base).
Strict all-target/all-feature Clippy, fmt and current-base bans/ratchets pass.
The inventory is exactly four removed history terminal hashes/occurrences;
all other keys/order and checker/STAGING are unchanged. Source/preservation
review, fresh CI and BingX CPU/WASM size remain separate readiness gates.

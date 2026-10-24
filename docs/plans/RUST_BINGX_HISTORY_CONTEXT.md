# BingX history paging on the original caller Read root

Status: plan first; original compiling assertions next, no production conversion.
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
| Fills | `/openApi/swap/v2/trade/fillHistory`; required symbol; default seven-day range; `startTs`, `endTs`, `pageSize`, pageIndex starts at 1; limit default 100, clamp 1..1000. Inclusive windows at most seven days, next begins at previous end+1 ms. A full mapped page increments pageIndex, max 1000 per window; no new identity or an out-of-window row rejects. Short pages advance the window or finish. |
| Orders | `/openApi/swap/v2/trade/allOrders`; optional symbol; `startTime`, `endTime`, `limit`, default 500, clamp 1..1000. Paired unequal bounds use the existing RangeWalk with seven-day chunks and a 50-page-per-chunk cap. No bounds use one response; equal bounds reject a saturated page. |
| Income | `/openApi/swap/v2/user/income`; optional symbol/incomeType; default 100, clamp 1..1000. Same RangeWalk and equal-bound saturation rules as orders. Income identity remains tranId/type/value/time. |
| Public trades | `/openApi/swap/v2/quote/trades`; one unsigned response, default 500, clamp 1..1000, native symbol and original response order. |

Retain RangeWalk's exact split/progress rules, its native ChunkDone loop, and
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

## Custody and direct cutover

After native local validation, a genuine remote query captures its caller wait
exactly once. Expired uses reject_expired_command(Read); missing/failed receipt
uses reject_unstamped_command(Read), with no fresh default in the venue.
Signed reads select then call command_root; unsigned reads use the existing
admit_caller_read_root(None, None). Root deadline is immutable through all
200 ms delays, seven-day windows, RangeWalk chunks and signature resends.

Retain root/selected snapshot and ReadCompletion in native state. Use
ReadHistoryRows for the custom fill pager and single-response path, with the
existing FillPages policy retained under validated row admission; use
ReadRangeHistory for the existing RangeWalk. No second authoritative row list,
coverage policy or terminal owner. Staging/filtering/identity/page acceptance
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

# Crypto.com unsigned PublicTrades Read custody

Plan-first base: actual landing 1e2458b1c98. Owner: codex-ih35js. Scope is
unsigned PublicTrades only; implement after/stacked on reviewed #2052, as ruled
by claude-ddi2il. This checkpoint contains no production changes.

## Exact inventory and ownership

The real checker records four fingerprints/five occurrences in history.rs:

| Fingerprint | Occurrences | Owner and removal |
| --- | ---: | --- |
| 073a9daacf5c0553bfb2650ddc8db7b942b5f6a0c8125261daff7479814953b7 | 1 | Unsigned PublicTrades request_streamed sender; managed submission removes it. |
| 7698d5bd23a40574165507b4b54b5d093fb00701512d29250192f1345e4da553 | 1 | PublicTrades URL format macro with the old request binding; extract into the exclusive public request path. |
| d702253b3d082dc8c7278bfe17bdb876eeda775fedd004ebaaeea6cdd16f24ad | 2 | Trades-kind checks in send and response; separate public authority removes both shared branches. |
| dfb17bdf89f16befd8bb7566ac05d0bddf2980c6627b642f20e1f1b5a4859508 | 1 | Private paging boolean witness; retain unchanged. |

Expected removal is three fingerprints/four occurrences, not just the direct
sender singleton originally reported. Do not retain dead public legacy branches
or two completion authorities to preserve counts. At the base Crypto.com has
12F/13O; after #2052 discovery it has 11F/12O, and this extraction leaves 8F/8O.
History itself becomes 1F/1O. The actual scanner decides the removal-only delta
after implementation; do not add or grow any fingerprint.

Open-PR file audit at the base: only own #2052 touches Crypto.com lib.rs and
shared test preparation; no other Crypto source claim. Original controls use a
new standalone test file and do not change existing fixtures. Future lib.rs
edits are restricted to a distinct PublicTrades Pending and History routing,
typed failure and exact-owner retirement arms; stack after #2052. No signing,
key-selection, engine, Cargo, private account/trading or combo-5b source changes.
Raw secrets and legacy private signing remain outside this unsigned adoption.
The signer owner confirms no full-custody private Crypto.com pin exists.

## Native contract

Query::PublicTrades bypasses the private credential requirement. Native time
range validation still precedes remote work; limit zero and equal bounds retain
the existing managed synchronous empty result. With positive limit the endpoint
is unsigned GET {rest}/public/get-trades?instrument_name=<encoded>&count=<n>.
Default requested limit is 25; wire count is min(limit,150). Both production and
testnet use empty headers/body, Public routing and no signer/auth evidence.
Unknown symbols are sent verbatim; no market lookup or startup requirement.

The endpoint returns one streamed result.data page. Preserve input order and
repeated trade IDs; truncate at the caller limit, never sort/dedupe or walk a
cursor. Preserve existing id parsing, side mapping, timestamp milliseconds,
price/quantity parsing and quoteQty=price*qty. A malformed row rejects the whole
unaccepted page; no accepted prefix or INCOMPLETE marker exists for this one
page. Existing history.rs and tests/history.rs public positives remain intact.

The native per-path next_by_path queue spaces submissions by 10ms across callers.
This is dequeue eligibility, not proof of release spacing after asynchronous
preparation. Preserve native eligibility and give the original root and each
child the same canonical Context Pacing group with minimum_gap_ms=10.0, so
actual releases also keep the bound. No venue release clock/ledger/timer is
added. Charge the native queue slot only after successful child submission;
engine pacing owns release reservations. Pin delayed preparation/claim plus
two callers in adoption controls before claiming physical spacing.
Transport failure has at most three retries/four requests, each after exactly
1000ms; failed streamed staging is discarded before retry. The existing 40101
native-response case grants one retry after 1000ms and stays unsigned. Other
HTTP/native failures are not retried. Do not introduce substring-based auth
health evidence or new status ladders; retain existing native policy, with any
required shared classifier decision reviewed before implementation. Public
40101 must never produce AccountAuth or increment auth health.

## Shared authority and callback shape

Use genuine admit_caller_read_root(None,Some(public-trades Pacing)) at entry, then
ReadCompletion::from_context plus ReadHistoryRows for the one response. One
immutable receipt deadline spans path-queue wait and all native retries; absent
host timeout is already stamped with the SDK default. Missing receipt refuses
through shared admission, never a local default/root or numeric fallback.

Mint each credential-free Read/20s/Never child via http_leg_context from the
original root at eligible native dequeue, capped by that root. Each physical
request uses request_managed and the same result.data stream location. Route
preparation is inside the child bound; no retry renews the root or reuses an old
physical delivery. Use actual ReadHistoryRows admission before parsing or
staging, before accepting a page, and before success/error response policy.
Use the original root for terminal completion. Retire each child before retry;
failed staging is discarded without accepting it. A denied local child or
submission is strict through that exact live root, with no copied authority
checks, new native timer or numeric fallback. Genuine revoked owners are inert.

Engine root retirement drops only that exact Owner's queued PublicTrades work
through read_root_retired; do not clear another command or reused numeric ID.
Root expiry is strict typed Timeout with no rows/marker/finalizer and no later
send, even while queued or parked. An expired physical child under a live root
retains native transport retry policy, bound by the root. Matching typed failure
owns cleanup only, never DATA. Uufub4 API-fit confirmation is required before
production; this plan does not claim the retry composition is already reviewed.
Private CursorHistory and signed calls retain their existing implementation.

## Original controls before source

Commit this plan, then add compiling real-Engine controls on the unchanged
PublicTrades sender. Record each genuine assertion failure separately from
working native controls. Never claim compile failures as RED evidence.

1. Public preparation blocks physical release; exact unsigned URL/count/network,
   routing and real prepare/bind/claim stay pinned.
2. Explicit short caller wait caps a child, including route parking and native
   10ms queue wait; both expiry ingress orders, equality and late readiness deny
   data and terminal reuse. Missing clock and genuine owner capacity deny work.
3. Root retirement while queued or waiting for retry prevents sends; unrelated
   live callers survive. Duplicate live numeric IDs cannot create a second read.
4. Stream a live prefix then end/fail at the deadline; no partial rows/marker.
   Reused ID plus stale old response cannot settle the new owner.
5. Native positives: exact escaped wire on both networks, default/large/zero
   limit, duplicate IDs/order/financial values, malformed whole-page failure,
   10ms queue cadence, transport four-attempt schedule and prefix reset,
   unsigned 40101 one-retry/no-auth, other native refusal/no retry.

After review, implementation is a separate checkpoint. Prune only the three public fingerprints/four occurrences;
run scoped Crypto.com tests and strict Clippy sequentially with CARGO_BUILD_JOBS=4,
fmt, request checker/test pair and bans. No local workspace/WASM/replay sweep.
Independent source/adversarial, caller and full-map reviews are required. Push a
small draft and let CI certify family bytes/CPU/replay. No merge without GO.

Architecture gate: No if shared entry/root/leg/row algorithms suffice. If native
retry composition needs a new engine seam, stop and report before source rather
than add a per-venue issuer, lifetime, finalizer or retry-authority algorithm.


## Bounded API direction

Uufub4 confirms original-root/new-child composition on actual 1e2458b1c98 fits
without Plan. Failure-only consumes the returned Pending, validates the live
root, retires the old child even at physical expiry and resets failed staging
before a native retry. Preparation failure and root expiry are strict. This is
API direction, not source or executed-control clearance. One exclusive public
Read authority is required; use a distinct PublicTrades request/Pending instead
of retaining legacy and managed terminals for the same public query. Native
private authority remains in its existing closed branch.


## Original executed checkpoint

The dedicated real-Engine target compiles and runs 17 controls on unchanged
1e2458b1c98 PublicTrades production: 8 assertion RED / 9 working GREEN / zero
ignored. REDs are eager route release, root equality DATA, absent root timer,
live streamed prefix at expiry, expired native queue dequeue, retry root renewal,
duplicate live owner and shared capacity bypass. Timer-first fails first at the
missing caller-bound timer; later inertness assertions are adoption gates, not
independent original evidence. The queued/retry/duplicate/capacity controls fail
first at actual physical HTTP where none should be allowed. Invalid clock is
already a working shared boundary control, not a new defect.

Working controls pin both exact hosts/escaped wire/default/count clamp, duplicate
rows and financial values, zero limit, native 10ms eligibility, four transport
attempts with exact 1s waits, failed-prefix reset, one unsigned 40101 retry with
no auth evidence, malformed-row rejection and other API refusal. An initial
combined positive omitted result.data from its API-refusal envelope; the native
stream parser correctly treated that as transport failure and retried. Corrected
the complete-refusal fixture and added a separate working missing-array retry
control. This fixture mismatch is not counted as a RED or a production defect.
Malformed envelope/stream retries retain native transport treatment; malformed
rows within a valid array still reject the unaccepted page without prefix.

No existing source, test, fixture, checker, inventory or Cargo file is changed.
Only this plan/index and the new dedicated test differ from the base. Original
assertions remain identifiable for implementation; no ignored tests or test-only
venue production path is added. Production awaits bounded reviews and the #2052
stack. Family performance/bytes and broad CI are not claimed by this checkpoint.


Scoped strict Clippy on the new target passes. The normal inventory check
currently fails only on foreign WEEX depth_query.rs/lib.rs because the remote
landing advanced past this intentionally unchanged original base. The manifest,
all Crypto production and existing fixtures remain exact 1e2458b1c98; no checker
override or foreign prune is used. Integrate the actual tip and reviewed #2052
only after recording this original checkpoint, then rerun normal checks.


## Review findings and native text-collision boundary

Source/API review is CLEAR at 0028d33a2a8 with two explicit adoption gates:
real physical-child Timeout under a live root must retire/reset and retry;
real preparation refusal must terminate strictly without entering native
transport retry. Added original controls for both (preparation is necessarily
gated first by absent route preparation in unchanged production).

Caller review found native history_failed tests error.contains("40101") before
transport policy. Thus code40001/message containing40101 retries once, and a
transport message containing40101 gets one retry rather than three. Added
original controls pin both cases. They are baseline evidence, not an instruction
to copy substring routing into the new managed branch. Production is held for
an explicit retry-policy ruling: shared native-code-only classification versus
preserving that legacy projection. Neither may silently change. Private history
continues unchanged. This also qualifies the earlier broad statement that other
HTTP/native errors never retry: it excludes this legacy text collision.

Full-map review verified untouched production and identified the incomplete
singleton projection; the corrected map names the genuine 3F/4O public
extraction. Its README blank-line finding is fixed by appending the row directly
to the table.


## Review closure and overseer ruling

The larger removal-only map (3F/4O) is approved. README now appends its row
directly without splitting the table. No scope beyond the exclusive public
authority is added; private cursor and signer behavior stays unchanged.

Retry ruling: preserve legacy counts exactly, including both text-collision
cases. Use typed native code/message and typed transport/preparation distinction.
Where those fields cannot express the legacy decision without text matching,
retain only that minimum compatibility check, name it as a known CALLERS
follow-up, and never infer auth health from it. No copied status/text ladder or
changed retry count is authorized. Preparation remains strict and native released
transport/physical-child Timeout remains retryable while the root is live.

The expanded original target executes 21 controls: 10 assertion RED / 11 GREEN /
zero ignored. Added REDs are preparation refusal (first gate is the absent
preparation path) and the absent engine-owned physical-child deadline. The
physical-child case intentionally fails at its missing 20s engine timer on
legacy source; after adoption it drives that actual typed expiry and recovery,
not a synthetic HttpFailed. Two added native text-collision controls pass.
The initial 17-control receipt remains historical and is not overwritten.


## Managed implementation checkpoint

PublicTrades now has its own module/request/Pending and exclusive
ReadHistoryRows authority; private Kind::Trades/row/sender branches are removed.
The shared original caller root is issued before wait, native queue eligibility
remains separate from shared final-claim pacing, and each 20s child comes from
that root at dequeue. Matching failure-only callbacks retire the old child and
reset unaccepted staging before the native retry; preparation refuses strictly.
The shared classifier supplies read kinds/native fields. The approved minimum
40101 compatibility predicate keeps both pinned collision counts, is named in
CALLERS, and supplies no auth-health inference. Original controls are unchanged;
four extra managed controls pin real delayed claims, expiry/readiness, numeric
reuse with old DATA and typed public auth metadata without health evidence.

The first compile caught unused legacy imports and a missing state initializer;
those are corrected, not RED evidence. The first managed run then exposed a
missing canonical pacing-group declaration; registering the same Context group
in Venue::http_pacing made all21 original controls pass. No engine source or
venue release clock/ledger was introduced. Scoped suite/strict and broad CI
receipts are recorded separately after execution.


All25 dedicated controls and the scoped Crypto.com suite216 tests pass. The
older mixed private/public dispatch fixture now uses real prepared_actions
before inspecting HTTP; its no-managed-actions assertion remains after actual
host preparation, and every wire/result assertion is retained. No financial
assertion was weakened. The extra four managed controls are not original RED
evidence. Original21 assertion bodies remain exact to the original checkpoint.

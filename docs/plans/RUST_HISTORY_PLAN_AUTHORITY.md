# Plan and Slot history authority

Status: plan and original assertion controls; exact API review required before
runtime source. This is a separate engine follow-up to original-root history bridge #1848, landed as
`6872554ac85244ec7b768b79af7187013dba429a`. Its ReadCompletion constructor and
original Read-root authority remain unchanged. Plan and Slot are opaque engine
owners, never original-root Context substitutes.

## Consumers and ownership

The first consumer is BloFin private general history and closed positions.
claude-4yjlw0 owns the signed-secondary dispatch migration and native recipe;
codex-9czdjh owns unsigned/PublicTrades and the native history contracts.
Their regions are history.rs and closed.rs, with lib.rs callback integration
after the PublicTrades consumer releases it. Do not edit those regions in this
engine PR. Coordinate background/mixed-leg dependencies with codex-hcmq3f.

BloFin uses signed Secondary reads with passphrase fallback. The history and
closed-position page caps are both 100; the account orders census's 50-response
bound and continuation policy do not apply. Keep each query's native geometry:

| Family | Continuation and terminal behavior |
| --- | --- |
| Closed positions | Page size 100; continue only when raw count >= 100. End on no new rows or unchanged cursor. A page cap or cursor cycle retains accepted rows with an owned incomplete marker. |
| General Income | Continue when raw count >= 100 and the requested start has not been reached. Missing/repeated cursor or page-budget exhaustion rejects strictly, even after accepted rows. |
| General Fills | Continue when raw count >= the clamped requested count, selected accepted+staged rows are still below that count, and the requested start has not been reached. Filtered rows can require additional pages; preserve strict cursor/budget rejection. |
| OrderHistory | One response; the requested limit controls wire parameters only. Do not introduce pagination or local truncation. |

Current wrapper behavior matters: general history and closed positions call
CursorHistory::finish/finish_with (landed history.rs:503/515), which delegate to
HistoryRows::finish/finish_with (history.rs:323/336). A later-page
Err retains accepted rows and emits an owned incomplete marker; no accepted
rows means rejection. The failed page's staged rows never enter that result.
Primitive CursorWalk failure alone does not describe query completion.

There is no local HTTP retry loop in these command histories. A later failure
terminates or recovers accepted rows; never borrow account census retries or
startup policy. The native owners explicitly approve this declaration:

- Per call: 20,000 ms, one attempt, RetryPolicy::Never and no added pacing.
  This matches engine.rs's legacy request default and existing managed BloFin
  reads. It includes actual shared preparation/release time; queue or signing
  delay cannot renew the call or root.
- OrderHistory: one endpoint, one serial slot and a 20,000 ms root. Its wire-only
  limit does not create additional slots or local truncation.
- Income, Fills and closed positions: one endpoint family per query, up to 100
  serial native steps with one call each, giving a checked worst-case
  2,000,000 ms root. Each family's earlier-stop rules still decide actual sends.
  Fills without a symbol retains its no-I/O empty result, and unsupported
  OrderByClientId remains a no-I/O refusal.

The per-call 20-second default already exists. The explicit whole-query root is
new: native legacy history previously had no whole-query deadline. The BloFin
consumer's CALLERS entry must describe that new root-expiry boundary alongside
strict closed preparation refusal. Before consumer readiness, inventory the
actual NX/Agent outer wait bounds and abandonment path with their owners. State
whether callers stop waiting sooner and what they observe; distinguish a local
wait timeout from an engine observer detach or command cancellation. Do not
claim either cancels engine work without an executed control. This consumer
evidence remains open; the engine root is not a promise that a caller waits
for its full duration. Test final-slot physical timeout versus root
expiry precedence: root expiry never permits retained success. Do not tighten
these declared bounds or add retries without another explicit owner decision.

Managed signing/preparation refusal must be strict and typed, with no retry or
auth inference. General history already rejects header preparation strictly.
Closed preparation currently uses legacy finish and can recover accepted rows;
changing it to strict managed rejection is the approved caller-visible change,
not a claim about current behavior. Its native owner must pin the original RED
and the changed behavior, including no retained result or incomplete marker,
and document it in CALLERS in the BloFin consumer PR, not this engine PR.

AsterDEX and Lighter need the same shared authority later. Their native range,
stage, timer and strict-financial contracts remain separately planned. This PR
must not imply venue adoption or manufacture a uniform native retry policy.

codex-cxxyqr owns the history API/core/tests. codex-uufub4 reviews lifecycle API
fit and any narrow registry/Plan helper. Existing Plan terminal-funnel owners
review settlement changes. No shared allocator, journal, timer, observer table,
executor or credential policy is duplicated here.

## Current source facts and missing seams

At the landed base, `request_context_planned.rs::validate_plan_delivery` checks
exact released DATA, including physical Attempt and live selected call. It does
not admit failed deliveries as data. `finish_plan_call` settles that exact DATA
slot. `done_read_plan_after_data` permits early Read completion from the exact
current DATA slot while retiring unissued remainder. `done_plan` requires the
schedule complete. `finish_plan_failed_call` currently accepts Mutation only.
The shared prepare/commit Plan terminal funnel was established by #1894.

These APIs cannot yet express accepted-prefix history success from an actual
later Read delivery failure. Do not invoke the Mutation helper, pretend failure
is DATA, call numeric completion, cast Plan to Read, or finish through a local
venue adapter. A narrowly checked Read recovery-terminal seam is an explicit
API gate. Plan construction, slot selection, pacing and retries stay with the
existing Plan APIs and their declared credits.

## Proposed typed history surface

Introduce a sealed PlanReadAuthority and separate PlanCompletion,
PlanHistoryRows, PlanCursorHistory and PlanRangeHistory types behind the existing
default-off `http-plans` feature. Names and signatures are proposals for API
review, not frozen APIs. Reuse the shared row, paging, identity and coverage
cores; do not copy venue parsers or make ReadCompletion accept Plan handles.

Construction validates the exact live engine-issued Read Plan, its command
identity/generation and supported owner kind through the shared registry. It
retains immutable Plan custody without reserving resources or granting send
rights. First-slice proposal: command-owned Read Plans only. Execution-owned
Plans and per-observer incomplete-marker projection require a separate explicit
agreement with the group owner and actual execution-Plan seam; they must refuse
rather than receive fabricated command IDs. Decide this scope in API review.
Mutation Plans, foreign issuers, retired Plans and mismatched commands refuse.
Duplicate bindings reserve nothing and only the first authorized terminal wins.

Every page operation takes the exact Plan Slot, physical Context and current
Attempt from engine delivery custody. Slot identity includes its issuing
schedule; no caller ordinal or request number is authoritative. Bind one staged
page to this tuple. Sibling calls, another slot, a reselected physical attempt
or a reused schedule cannot accept that page. State refusal changes neither
staged rows/identities nor coverage or the engine schedule.

Retain #1848's pending-identity discipline: identities commit only with an
accepted page, failed prefix identities are discarded, and discard/restart and
accepted page transition clear page custody. Range-derived coverage facts need
the same delivery guard. Command-bound pre-send geometry selection remains
separate from response-derived admission.

### Page acceptance and exact settlement

Page acceptance and Plan-slot settlement must be one checked synchronous
operation. All fallible page validation and exact delivery/slot checks precede
both accepted-row publication and engine settlement. A rejected page cannot
settle a slot; a refused settlement cannot leave rows accepted. There is no
caller-provided settled flag or second slot table.

Use the #1894 prepare/commit settlement mechanism with a private one-use token
retaining exclusive mutable Cx custody, or an exclusive mutable Cx guard if the
lifecycle owner selects that signature. Prepare accepted-page storage before
any infallible engine/history commit. A token may not release its borrow or
expose Cx before commit. Pin that no Cx effect can interleave between slot
settlement and page acceptance. Do not weaken the original Read guard or add
unchecked mutator twins. Diagnostics happen after dropping custody; further
staging requires fresh admission. Native next-slot sends and timer continuation
still use shared Plan authority.

Final-page DATA is a distinct prepared terminal/accept handoff. When unissued
slots remain, done_read_plan_after_data must run before finish_plan_call. Do not
settle the final call and then attempt that invalid early terminal. Prepare the
page storage, finalizer result and terminal/marker resources under the same
exclusive custody, then commit acceptance and the shared early Plan seal with
no interleaved or fallible effect. Ordinary continuation settles once; early
completion performs its own exact settlement once. Selected peers cannot be
retired. Exact signatures for both handoffs remain an API gate.

### Terminal and failure contract

Terminal validation occurs before captured finalizers, incomplete markers and
Plan settlement or seal. Reuse the one shared prepare/commit Plan terminal
funnel; no fallible projection/storage work follows a published marker. A denied
owner produces no marker, rows, codec/finalizer effects or terminal and cannot
retire a different current slot. No numeric retry follows an authority denial.

Successful final DATA may retire only unissued remainder under existing Read
conditional-completion rules. A later actual released transport failure may
produce native accepted-prefix recovery only through the new reviewed Read
failure seam. It must prove the exact latest released failed Slot/Context/Attempt,
an engine-recorded transport cause, retained owner authority, and the history
core's accepted-only evidence. Active { failed: true } is insufficient because
provider and preparation failures can set it too. No caller error text, kind or
boolean supplies that proof. Failed
page rows/identities are excluded before retry or settlement. First-page failure
and zero accepted filtered rows do not manufacture success. The first missing
coverage cause and native final sorting/filtering/limits remain intact.

Preparation/signing/policy refusal stays strict even after accepted rows: no
retained result or incomplete marker, and supplied typed error fields pass
through without response/auth reinterpretation. Ordinary native errors follow
the consumer's existing strict-versus-recoverable distinction. Root expiry and
physical attempt expiry are distinct. Root expiry grants no retained-result
recovery permission; physical failure before a live root expires remains subject
to exact engine failure-cause validation. No new lease, deadline extension,
retry credit, send permission or failure-to-DATA conversion is introduced.

## Original REDs and required controls

Commit compiling semantic assertion REDs against the landed source before the
implementation. Exercise genuine command entry, Preflight, Plan admission and
real Prepare/Bind/Claim rather than fabricated Contexts or registry internals.
A missing proposed API compile error is not original RED evidence. A legacy
numeric Completion under a genuine Plan is a negative reproduction fixture
only; it cannot become a production fallback. Preserve original assertions.

### Runnable landed-source reproductions versus new API controls

Before source, run and commit a genuine two-slot Read Plan fixture using only
landed APIs. Accept the first released page, select/release the second slot and
inject Event::HttpFailed for its current request. The reproduction retains the
legacy numeric history owner only as a test seam: require exactly one Plan-owned
partial terminal containing only the first page and its owned marker. Record
actual output; the missing Read failure terminal must appear as an assertion
failure, not as a missing-method compile error. A companion proves that current
finish_plan_failed_call refuses Read and that first-page transport failure has
one ordinary Plan failure and no rows/marker. These are separate assertions;
the refused Mutation-only helper is never used as a production workaround.

A second landed-source negative fixture retains a numeric Completion after its
Plan seals, then tries to publish incomplete coverage from a stale callback.
Require no marker or second terminal, with a live other owner as the positive
control. Preserve terminal/marker assertions when the fixture seam moves to the
new authority. Neither reproduction grants production numeric fallback.

Constructor, opaque Slot admission, prepared accept/settle and new failure-cause
API shape controls are implementation tests after signatures freeze. Do not
claim they were executable REDs before that API existed. The BloFin closed
preparation-refusal RED is a separately identified native-lane receipt. All
constructor and custody controls remain unexecuted until the API exists. The
landed-source reproduction results are recorded below.

### Required implementation and consumer controls

- Two accepted pages on distinct serial slots terminate once with native-shaped
  rows through Plan custody; first page alone never completes the query.
- Staging then failing a page excludes its rows and identities. Reselecting the
  same Context with a new Attempt refuses admission until explicit discard;
  discard plus the current attempt succeeds with only its own rows/identities.
- Wrong Slot, foreign schedule, sibling Context, old Attempt, reused command,
  duplicate terminal, foreign issuer and expired owner are side-effect free.
  A live peer slot remains usable after each wrong-custody probe.
- Strict preparation/provider refusal after a full accepted page yields the
  exact typed failure, no result/marker, no retry and no auth/deactivation event.
  It cannot enter recovery even if the Plan call has Active { failed: true }.
- Real later-page transport failure recovers accepted rows only where native
  policy permits; first-page and zero-accepted-row failures remain failures.
  Failed delivery never authorizes new rows, coverage or Store publication.
- A final short page accepts rows and completes early with unissued slots still
  reserved, without a prior finish_plan_call. Ordinary continuation settles once;
  selected sibling work cannot be skipped. Marker/finalizer/storage refusal leaves
  Plan/rows untouched and emits no marker.
- Cursor, range and multi-stage completion retain accepted-stage baselines,
  dedupe and the first coverage gap. No timer can admit response data; only
  shared Plan authority may permit delayed next-slot sends.
- Feature off preserves original-root/legacy behavior and excludes the new API.
  Mutation/unsupported Execution ownership is explicitly refused until reviewed.

Standalone BloFin must explicitly enable engine `http-plans` in its dependency
graph before adopting this authority; workspace feature unification is not
proof. Require isolated BloFin feature/dependency checks, standalone family WASM
size and its replay CPU gate (BloFin is replay-gated), plus existing correctness
and memory gates. Report a size/CPU failure rather than waiving it. Engine
feature-off checks must keep the new surface excluded.

Capacity controls compare complete typed errors captured from genuine admission,
not literal capacity text. #1877 changed the display text to `Engine request
capacity reached`; that does not alter authority or the expected typed shape.

Before source, uufub4 must clear exact page/settlement and Read failure-terminal
contracts and both BloFin lanes must confirm native recovery and bound inputs.
Independent source/caller reviews follow implementation. Require a real managed
paged consumer receipt, engine feature-on/off and scoped history controls,
strict formatting/lints/bans/scanner checks, normal CI/WASM replay/size gates,
actual prerequisite integration, explicit engine window, fresh separate guard
and head-matched merge. Report a missing shared primitive or failing native
contract rather than substituting venue policy. Update CALLERS only for the
behavior actually activated by the implementation; this plan has no production
caller change.


## Native contract verification for plan review

Independent focused runs on c69cccb8390 (source identical to actual #1848)
passed the existing general later-income accepted-only recovery control,
general first-page rejection matrix, closed second-page accepted-only recovery,
and closed first-page rejection. The general later-failure matrix includes an
actual HttpFailed case. This establishes the existing wrapper contract; it is
not a new Plan-authority RED or a private managed consumer proof. Both BloFin
reviewers agree on accepted-only later recovery and no local retries. Source and
new API implementation remain gated on exact lifecycle signatures. The native
20-second call and per-family whole-query bounds above are now explicitly
confirmed by both BloFin owners; they are declarations, not an executed managed
private-history consumer proof.


## Executed original Plan-history assertion controls

The test-only history_plan_bridge target compiles against unchanged runtime
source from actual #1848. With http-plans,test-support enabled, three controls
execute: one first-page failure control passes and two semantic assertions fail.
The two-slot fixture uses genuine ReadOnly Preflight/Plan admission, shared
prepared_actions for Prepare/Bind/Claim, exact DATA validation and real HttpFailed
delivery. The Mutation-only failed-call helper returns Invalid as a separate
working control; it is not a workaround.

After one accepted page and the second page's Network failure, there is one
owned marker and a terminal, but no successful retained-row terminal (expected
[1], observed no CommandDone). A sealed numeric completion fixture produces
one additional stale terminal and one marker where both must be absent, while
a different genuine live Read owner still completes successfully. Those are
assertion REDs, not compile or setup failures. The numeric owner exists only in
the original negative test seam and must be replaced by opaque Plan authority;
all terminal/marker/result assertions remain when that seam changes. No new
runtime authority or exact API-signature readiness is claimed.

Independent reviews of original checkpoint `27a68bef107` are CLEAR from
claude-8x5tm1 and codex-uufub4 for original evidence only. The independent
receipt command is `cargo test -p safe-cex-engine --all-features --test
history_plan_bridge --jobs 2`: one GREEN control and two assertion REDs. The
target is gated by both http-plans and test-support; a feature-off run executing
zero tests is not this receipt. Marker detection uses HISTORY_INCOMPLETE plus
command-field membership so additional diagnostic fields cannot hide a stale
marker. Runtime signatures, consumer outer-wait evidence and adoption remain
open gates.

After marker-predicate hardening, the same all-features target was rerun:
one control passed, the accepted-prefix assertion still observed no CommandDone,
and the stale-owner assertion still observed one terminal and one marker.
All three test bodies and expected terminal/marker values are unchanged.

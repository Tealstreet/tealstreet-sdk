# WEEX history paging on the original caller Read root

Owner: codex-cxxyqr, transferred from codex-uufub4 by the overseer.
Original RED baseline: actual `ba3d7da1d3aaba26d6c40d6615fdc467d6ec185d`.
Current runtime integration: actual `c55cd23074d` (plain merges preserve all incoming source).
Branch: `feat/rust-weex-history-context`. Status: source checkpoint; readiness review pending.

## Scope and dependencies

Convert `history_next_list` and `history_income_request`, their response/item/
failure/timer handoffs, and their retained history owners. Use landed #1848
ReadCompletion/history admission and actual #1945/#1976 caller receipt and root
expiry. No Plan, Background root, venue root timer, numeric terminal, deadline
renewal, or shared engine edit belongs to this slice. Existing synchronous
history admission/refusals keep their already-managed local path.

The signed-page adapter consumes actual #1983 `a2533f5761f`, owned by
codex-9czdjh: `SelectedRead::command_root`, immutable `ReadRoot`, and move-only
`ReadLeg`. It was plain-merged at `9fd6e5afe47` before the source checkpoint;
actual landing `5f86ae51717` is now integrated at `57b51ff5bd3`. The source uses
the landed root/leg seam directly, without a venue substitute. #1992's `scoped`
addition is not this seam.

Keep #1997 account-cycle work on its separate branch. Shared WEEX lib routing
edits must compose with its owner; `send_account`, trading, socket custody,
managed.rs, sign.rs and engine sources are outside this history slice.

## Native geometry and selection to preserve

| Family | Native physical work and completion |
| --- | --- |
| Fills | GET `/capi/v3/userTrades`; sorted encoded query; 7-day windows, at most 1,000 windows, one response per window; limit clamped 1..100. |
| Orders | GET `/capi/v3/order/history`, then `/capi/v3/allAlgoOrders` per 90-day window, at most 1,000 windows; one response per endpoint, normal page=0. Algo hasMore records existing incomplete coverage rather than adding pagination. |
| Public trades | One unsigned GET `/capi/v3/market/trades`, limit clamped 1..1,000, preserving native response order. |
| Ranged income | Primary-only signed POST `/capi/v3/account/income`; exact existing JSON body and items stream; 100-day RangeWalk windows, split threshold/page_limit=100 rows, max_pages_per_chunk=50 and lifetime request budget 10,000 physical requests; limit=100. Preserve 100 ms PAGE_DELAY after each page and ChunkDone, and pause eligibility. |
| Single income | Same primary-only POST, one response, native limit clamped 1..100. Existing hasNextPage/nextKey produces incomplete coverage. |

Preserve the existing primary-presence local gate before signed command_root:
fills, orders and income return `[]` without HTTP when primary is absent, even
if a secondary read key is configured. A secondary-only account must not begin
new signed history work. The local client-order-id refusal also stays intact.

After that gate, GET private lists select ReadRole::Read (secondary if configured, otherwise
primary), scope None, reproducing native GET selection. Income uses
ReadRole::Primary { reason: PrimaryReason::History, scope: None }, reproducing
native POST primary selection explicitly. Do not generalize POST to secondary.
Primary/secondary passphrase fallback remains trusted setup's existing rule.
Select once before the signed root and retain the immutable selection across
all pages, windows, endpoint changes, pauses and timer callbacks. No new account
auth evidence is attached: native history has no account-read scope tag.

Each physical call keeps the effective legacy 20,000 ms timeout, one attempt,
RetryPolicy::Never. Do not introduce retries, shrink native geometry, or derive
a whole-query deadline from page count. The original raw caller receipt cap is
the only root bound; absent host timeout defaults only at external receipt.
Overrides above 120,000 ms remain effective. Preparation and queue delays use
the same immutable root and do not create another caller wait.

## Custody and completion

1. After existing local query validation, observe command_wait. Expired uses
   reject_expired_command(Read); None uses reject_unstamped_command(Read), with
   no guessed default. Signed queries use SelectedRead::command_root once;
   public trades use admit_caller_read_root(None, None) once.
2. Construct ReadCompletion::from_context with that exact original root; use
   ReadCursorHistory and ReadRangeHistory throughout, without legacy aliases.
   Retain the root/selection alongside the native Read state. Only completion
   authority, not the numeric command id, permits mutation or terminal effects.
3. Mint a fresh 20 s child only at eligible send, after native pause and pacing.
   Signed children are ReadRoot::leg and ReadLeg::queue with the existing WEEX
   dispatch::Http recipe, exact path/query/body and stream selector. Unsigned
   trades use http_leg_context and request_managed_with_headers with the native
   empty header vector. No child is minted while parked. Keep the history pause
   gate before public get; the shared get helper also has deferred_gets, so any
   history continuation retained there must be removed by exact owner on root
   expiry. Never mint a physical child before either parking boundary.
   A local child-mint or queue-admission refusal strictly rejects the exact live
   root with its typed Read error, retiring its read and pacing state; it never
   enters accepted-prefix recovery. Only proven already-revoked owners drop
   inertly. Add controls for both refusals after an accepted page.
4. Each Pending retains its exact leg and original Owner generation. Check the
   exact current owner and delivery before any item decode, skipped-row event,
   raw-row counter, envelope codec, identity, staging, coverage or publication.
   The history admit guard binds the exact Context and Attempt before fresh,
   stage and advance/accept. A sibling or retired page grants no authority.
5. Preserve existing stage restart, accepted identities, filtered-row rules,
   sort/final shape and coverage text. Finish the old physical Context after
   accepting that page and before sending/parking the continuation. Keep the
   logical root live until the one history terminal.
6. A live-root native/transport failure preserves accepted-only recovery and
   one command-owned INCOMPLETE marker only when accepted rows are nonempty.
   With no accepted rows it rejects on any page: empty earlier windows do not
   turn a later failure into an empty partial success or create a marker.
   Failed/staged rows and identities never enter the finalizer. Provider or
   preparation refusal is strict even after accepted pages, retaining the exact
   typed VenueError with no retry or auth inference. Read entry/cap failures use
   canonical Read projection (outcome None), not fabricated NotSent.
   Preserve native REQUEST_FAILED notice before non-strict failure routing.
   Preserve global 429 rate::delay/paused_until handling before failure routing,
   including a page that subsequently fails; it remains a shared egress pause.
7. Root expiry is engine-owned strict Timeout, including equality: no rows,
   marker or finalizer. Implement read_root_retired(Owner, Vec<Pending>) without
   Cx: remove only the matching-generation read and parked continuation; drop
   handed-back pending pages. Do not touch another command or recycled id. A
   stale local pacing timer finds no work. No numeric completion in cleanup.

## Original REDs and preserved controls

Commit compiling tests before runtime; keep their assertions unchanged when
the implementation switches the managed release adapter. Separate every family
or timer-order variant so an early assertion cannot hide later evidence.

- Signed GET lists and primary-only POST income must emit Prepare, then Bind /
  Claim before HTTP; legacy release is a genuine assertion RED on this source.
  A native payload/selection positive proves the fixture signs real bytes.
- Accept one normal-order response, then leave algo in flight: caller root
  equality must yield one typed Timeout, zero partial success/marker. Compare a
  live-root later-page Network failure yielding only accepted normal rows plus
  one owned marker. Public trades and income each get independent root tests.
- Expire ranged income while parked at PAGE_DELAY or paused_until; no next
  request, codec, finalizer or numeric terminal after expiry. A live unrelated
  query continues, including when the expired numeric id is reused.
- Explicit caller waits, default wait and raised wait retain one deadline;
  fractionally live root is live until exact equality. Page timeout under a
  live longer root remains the native recovery path. Late DATA, preparation,
  Bind and a previous generation's timer cannot mutate current work.
- Provider refusal after accepted rows is strict and exact typed, with no
  auth rejection/deactivation. Disarmed provider delegates to the same native
  recipe. A public read uses no signer. Primary/secondary and income primary
  selection stay covered independently.
- Preserve native negatives independently: three empty fills windows then a
  Network failure reject without rows/marker; secondary-only private fills,
  orders and income return `[]` with no I/O. A 429 keeps REQUEST_FAILED and
  parks a subsequent public request until its Retry-After boundary.
- Keep symbol, orderId and window filtering, newest-first sort except public
  trades, symbol|id and income_identity dedup, millisecond flooring and fill
  seconds conversion. Missing native fill id still emits ROW_SKIPPED and an
  INCOMPLETE gap. Existing native fixtures remain preservation gates.

Record actual RED counts and assertion locations; API-shape controls relying on
#1983 are absent-by-dependency until its actual pin, not ignored or claimed run.

## Inventory, caller behavior and verification

The two legacy physical wrappers are not tracked scanner members. Conversion
alone does not justify deleting a STAGING entry. Regenerate actual sites and
prune only removed legacy Completion/terminal hashes; no rehash, exemption,
checker-policy change or widening. Keep the WEEX lib.rs STAGING_BOOTSTRAPS map
unchanged until its complete independent exit conditions are satisfied.

The source commit's CALLERS entry must state strict engine root expiry versus
live-root accepted-only recovery, immutable configured caller wait and native
20 s physical pages. SDK default 120 s listener abandonment is not transport
cancellation. NX currently uses the TS path; inventory is snapshot evidence,
not an executed native WEEX/NX receipt. Do not claim wrapper cancellation or
engine detach from an outer Promise timeout.

Run focused WEEX history/local-history/provider tests, bridge controls, strict
all-target WEEX Clippy and fmt, scanner/unit checks and bans on the exact source
head. CI supplies repository fast-lane checks plus isolated WEEX WASM size and
replay CPU gates; report a miss. Draft stays non-ready while #1983 or original
REDs/readiness controls remain unmet. No merge without overseer GO.

## Original-source receipt

The new `sdk-rs/venues/weex/tests/history_context.rs` runs against byte-unchanged
`ba3d7da1d3a` production with the trusted native signer and real handle_command
receipt. Its drain drives real Prepare/Bind/Claim when emitted and separately
counts legacy Http releases; there is no production adapter or policy copy.

Executed command: `cargo test -p safe-cex-venue-weex --test history_context`
from `sdk-rs`. Result: **6 assertion RED / 2 GREEN / 0 ignored**. No compile,
fixture, missing-request, selection, wire-body or parsing failure counts as RED.

- Signed list, signed income and public release each pass their native request
  assertions, then fail `(legacy, prepared, claimed)`: `(1,0,0)` vs `(0,1,1)`.
- Public root equality emits no terminal instead of one typed Timeout.
- Accepted normal orders plus algo HttpFailed exactly at the caller deadline
  returns those rows through native recovery instead of strict root Timeout.
- Ranged income's genuine 100-row page parks on the 100 ms timer; firing it
  after caller expiry produces no root terminal instead of strict Timeout.
- GREEN: a live-root algo failure preserves the one accepted normal-order row
  and one command-owned marker; an empty public response finishes once within
  the declared caller bound.

These pin the pre-cutover behavior only. The draft must not merge this RED
checkpoint. Signed production and new root/leg controls wait for actual #1983;
all six assertions must turn GREEN through the real shared path before READY.

codex-uufub4 cleared bounded bridge/core fit at plan `a80c8ac5f7d`, including
the actual #1983 dependency, retained original root and exact page/attempt
admission. The child-mint/queue-refusal source gate above folds in that review;
it is not a runtime or CI clearance.

## Native review follow-up

claude-4yjlw0's native review corrected the accepted-empty rule, primary-presence
gate, RangeWalk split/chunk/request bounds, empty public headers and shared
pause/failure notice handling. The plan above includes those corrections.

Five added preservation controls pass on unchanged baseline production: three
empty fills windows followed by a fourth-window Network failure rejects with no
INCOMPLETE marker; secondary-only fills, orders and income each return local
`[]` without physical work; a 429 emits REQUEST_FAILED and parks the next public
query until Retry-After, then releases it with no headers and completes.
Expanded target: **6 original assertion RED / 7 GREEN / 0 ignored**. The six
original test bodies and expected failures are unchanged. These five controls
are native preservation evidence, not additional original REDs.

## Actual #1983 source checkpoint

The conversion retains one selected signed ReadRoot (or unsigned caller root)
alongside each native walk. Exact delivery validation precedes streamed row and
response codecs; ReadCursorHistory/ReadRangeHistory guards own identity, stage,
coverage and accepted output. Physical legs retire before continuation, while
the original root survives all windows. The no-Cx expiry hook removes only the
matching Owner generation's native state. Provider refusals retain their exact
typed error and reject strictly; physical failure while live keeps native
accepted-only semantics. Local mint/queue refusal is strict and typed.

All 13 original/new preservation controls and all 40 existing history fixtures
pass on the source checkpoint. Existing fixtures now drive real managed
Prepare/Bind/Claim at their event time. The 10,000-request income-budget fixture
declares a 2,000,000 ms caller wait so it tests the unchanged native budget and
100 ms pacing rather than the new default 120 s root expiry. The provider
preparation control now refuses through an installed real provider, preserving
the immutable selection snapshot instead of mutating trusted credentials;
managed timers may be emitted then cleared, but none may survive refusal.

The actual lexical inventory is unchanged: neither legacy history Completion
constructors nor the physical wrappers are scanner members. Scanner and its 15
controls pass without manifest/checker/STAGING edits. Further admission,
generation and timeout-boundary controls, independent source review and fresh
CI/replay/size gates remain required before READY.

The old local-history remote-fallthrough capacity control intentionally changes
with this cutover: remote paging now requires a retained root plus one physical
child. Full capacity refuses once; one freed slot admits the root but strictly
refuses its child with no leak; two freed slots allow the unchanged native wire.
The existing local-query precedence/capacity controls remain unchanged.


## Native readiness controls on the integrated production source

The current history_context target passes 23 controls: the original thirteen
bodies are unchanged, plus ten independent controls for the default receipt cap
across endpoint restarts, an override completing beyond 120 seconds, fractional
predeadline recovery versus equality, a real child-only timer, both equal-time
timer orders, generation reuse with a live unrelated peer, typed preparation
refusal and typed provider refusal with no auth evidence.

The additional native capacity control accepts a normal order page, parks its
algo continuation behind the existing 429 pause, then retains 255 genuine Read
peers. A genuine admission control captures the full Capacity error. Dequeue
must reject the history with that same typed value, no prefix, marker or HTTP;
freeing one peer then permits a new root plus child, proving the refused root
was released while the other peers stayed live. This control passes. It tests
child construction capacity; it does not claim a separate native queue-refusal
injection after successful child construction. The returned-Pending queue-error
branch retires the child and invokes the same strict root rejection; its source
custody and shared #1983 queue controls remain review evidence, not that native
execution receipt.

No shared API, signer recipe, account-cycle code, inventory or STAGING is changed.
ReadCompletion, ReadCursorHistory and ReadRangeHistory are named explicitly in
production so no legacy authority alias obscures the cutover. Exact native/source
review and fresh strict/CI/WEEX size/replay results remain required.

Native re-review at `50c676196ba` is CLEAR with three Low findings. The
non-expiry revocation control first proves a genuine parked Read, then one exact
root failure, and reproduces the stale native reads entry as one assertion RED
with no compile/setup failure. Delay cancellation is asserted after that failing
comparison and is not claimed executed until the fix. Remove the dead History
deferred_gets cleanup and make any subsequent history-admission refusal retire
the physical leg and take the strict typed root path instead of waiting for expiry.
The revocation fix turns that assertion GREEN and reaches the delay-clear and
stale-timer assertions. All four defensive admission branches now finish the
physical Context and use strict typed history rejection. The dead deferred_gets
history filter is removed. The refreshed focused suite passes 84 controls:
9 unit, 40 native history, 23 custody and 12 local/admission. Exact delta
re-review and strict/CI gates remain separate.

### Current landing composition

Plain-merged actual `ceae26c3795` after #1997. The sole merge conflict was
CALLERS append ordering; incoming entries precede the unchanged history entry.
Together the two conversions remove the final production callers of
`signed_request` and its `account::auth_scope` adapter. The first merged build
failed on those two orphaned helpers under `-D warnings`; both are removed, and
the old helper-only secondary fixture retains explicit identity/slot selection
checks. Native history and account managed-release controls remain in place.
The exact lexical inventory is regenerated with removal only.

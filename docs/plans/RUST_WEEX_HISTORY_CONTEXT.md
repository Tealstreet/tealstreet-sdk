# WEEX history paging on the original caller Read root

Owner: codex-cxxyqr, transferred from codex-uufub4 by the overseer.
Baseline: actual `ba3d7da1d3aaba26d6c40d6615fdc467d6ec185d`.
Branch: `feat/rust-weex-history-context`. Status: plan first; no runtime claim.

## Scope and dependencies

Convert `history_next_list` and `history_income_request`, their response/item/
failure/timer handoffs, and their retained history owners. Use landed #1848
ReadCompletion/history admission and actual #1945/#1976 caller receipt and root
expiry. No Plan, Background root, venue root timer, numeric terminal, deadline
renewal, or shared engine edit belongs to this slice. Existing synchronous
history admission/refusals keep their already-managed local path.

The signed-page adapter additionally needs #1983, owned by codex-9czdjh:
`SelectedRead::command_root`, immutable `ReadRoot`, and move-only `ReadLeg`.
The reviewed candidate is `598b5a09cc3`; it is not an actual landing at this
checkpoint. The landed SelectedRead has only single-leg command/background
issuers, and its private identity cannot be bound to a caller-root child by
venue code. Do not emulate it or remint a command root. Original-source REDs
can run on this baseline; production conversion waits for the actual #1983
landing and its owner's current pin. #1992's `scoped` addition is not this seam.

Keep #1997 account-cycle work on its separate branch. Shared WEEX lib routing
edits must compose with its owner; `send_account`, trading, socket custody,
managed.rs, sign.rs and engine sources are outside this history slice.

## Native geometry and selection to preserve

| Family | Native physical work and completion |
| --- | --- |
| Fills | GET `/capi/v3/userTrades`; sorted encoded query; 7-day windows, at most 1,000 windows, one response per window; limit clamped 1..100. |
| Orders | GET `/capi/v3/order/history`, then `/capi/v3/allAlgoOrders` per 90-day window, at most 1,000 windows; one response per endpoint, normal page=0. Algo hasMore records existing incomplete coverage rather than adding pagination. |
| Public trades | One unsigned GET `/capi/v3/market/trades`, limit clamped 1..1,000, preserving native response order. |
| Ranged income | Primary-only signed POST `/capi/v3/account/income`; exact existing JSON body and items stream; 100-day RangeWalk windows with native splitting, per-walk bound 100 and lifetime request budget 10,000; limit=100. Preserve 100 ms PAGE_DELAY and pause eligibility. |
| Single income | Same primary-only POST, one response, native limit clamped 1..100. Existing hasNextPage/nextKey produces incomplete coverage. |

GET private lists select ReadRole::Read (secondary if configured, otherwise
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
   trades use http_leg_context and request_managed_with_headers, preserving the
   existing static headers. No child is minted while parked.
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
6. A live-root later-page native/transport failure preserves accepted-only
   recovery and one command-owned INCOMPLETE marker; first-page failure rejects.
   Failed/staged rows and identities never enter the finalizer. Provider or
   preparation refusal is strict even after accepted pages, retaining the exact
   typed VenueError with no retry or auth inference. Read entry/cap failures use
   canonical Read projection (outcome None), not fabricated NotSent.
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

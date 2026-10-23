# Command receipt caps for Read Plans

Owner: codex-cxxyqr. Reviewers: codex-uufub4 (Plan core) and
claude-fe7lgm (#1945 receipt custody). Assigned as a separate engine PR after
#1945. Initial plan base: actual `a828ede7ebb`. Current stack genuinely includes
#1945's frozen `15df705c0b5`, announced by fe7/overseer, after the authorized
source-cleared `f7550a0c6d0` checkpoint. Status: receipt-cap implementation and original assertion controls; independent
source review and dependency refresh remain required. The only merge conflicts
were shared CALLERS appends/order: frozen incoming text retains all prior lines
once. Engine/host/receipt runtime is byte-identical to frozen #1945.

## Problem and invariant

`admit_plan_owned` in `request_context_planned.rs` currently computes the
whole deadline as `schedule.bound().deadline(self.now)`. That is the checked
native execution envelope from admission time, deliberately excluding earlier
prerequisite reads. It does not cap work at the caller's original receipt
boundary. Reusing a previously observed `Remaining(ms)` after JWT/preflight
work would start that duration again from the later clock.

For a command opting into receipt-capped Plan ownership:

```
receipt_deadline = deadline captured by #1945 at external command receipt
plan_deadline = min(native_bound.deadline(admission_now), receipt_deadline)
```

The receipt deadline is absolute and immutable. Internal re-dispatch, JWT wait,
preflight DATA, page/stage transitions, retries, re-signing and readiness cannot
renew it. A shorter checked native envelope stays shorter. Native step counts,
concurrency, retry attempts/windows, fallback declarations and per-call bounds
stay unchanged; do not reduce geometry to manufacture a caller cap.

This is a lifetime restriction, not send, retry, completion or history authority.
An opaque Plan/Slot never becomes an original Read root.

## Proposed small API and scope

API shape cleared by uufub4 and fe7 at `a2795215932`:
an explicit receipt-capped Read preflight entry operation (provisional name
`begin_preflight_with_command_wait(policy)`) returning the existing opaque
`Preflight`. It shares the existing genuine-entry transaction and issuer. It
accepts no numeric command id, public deadline, duration, caller bool or
caller-minted token. It refuses a WriteAfterReads policy in this first slice.
The existing native-envelope preflight mode is a separate declared contract,
not a fallback when receipt capture fails.

The cleared public signature is
`begin_preflight_with_command_wait(policy) -> Result<Preflight, CommandEntryError>`. Do not add two issuers or a second Plan allocator.
A missing, foreign, duplicate or expired receipt must never silently select the
uncapped mode or a fresh 120-second default. #1945 owns default stamping once at
external receipt and Fresh-only `reject_expired_command(Operation)`.

The new mode must feed the single existing `admit_plan_owned` implementation.
For command-owned Read Plans it applies the retained cap before any reservation
or irreversible transition. This first slice excludes execution-owned planned
groups and mutation Plans: their founder/observer lifetime and write journal
contracts require separate composition decisions. Reject a capped preflight on
those unsupported admission routes atomically before consuming the preflight,
changing its phase, reserving credit or allocating a group.
Existing mutation and uncapped planned-execution contracts remain unchanged.

The explicit opt-in mode is preferred over automatic changes to every Read
Plan. Native-only existing admission remains unchanged, with no uncapped
fallback on a failed receipt capture. Exact signatures and the frozen dependency
remain gates before source.

## Capture and retained custody

#1945 keeps `Io.command_waits` stamps as `(receipt_sequence, absolute_deadline)`
per command id, and `Entry.receipt` identifies this genuine receipt. Its public
`command_wait()` answers only in genuine entry callbacks. `Waits::deadline` is
currently test-only; this PR owns the smallest necessary internal accessor once
#1945 is frozen.

Promote the existing `command_wait()` lookup into one private Io helper that
returns `(receipt, deadline)` only when `stamps[entry.command]` matches
`Entry.receipt`. Both `command_wait()` and cap capture use that helper; there is
no second lookup or independent receipt-matching rule. No accessor change is
required in #1945 itself.

Capture during the genuine entry transaction, before asynchronous work:

1. Require the entry's own live receipt and exact sequence. A duplicate id's
   entry cannot borrow another command's deadline or retire its stamp.
2. Obtain the original finite absolute deadline directly from the engine's
   receipt state; do not reconstruct it from an integer Remaining duration.
   Missing stamps always refuse the capped route: duplicate live id with no
   `Entry.receipt`, invalid/non-advancing default receipt clock, or exhausted
   checked receipt sequence. None selects uncapped mode or a new default.
3. Bind private receipt identity/deadline data to the newly admitted Preflight
   issuer and command generation. Capture and preflight ownership succeed or
   refuse atomically. Store no independently addressable receipt table here.
4. Retain that data in the existing preflight owner. JWT/preflight callbacks
   use the opaque Preflight plus exact native handoff custody; they do not need
   or fabricate a fresh command entry. Admission from that callback is normal.
5. At Plan admission, validate that exact preflight owner/generation, compute the
   checked native envelope normally, take the minimum with the retained cap,
   and refuse an expired cap before Plan/slot/timer/id/credit allocation.
6. Move the cap into the existing Plan owner; the handle's root and all slot
   windows use the resulting immutable deadline. There is no later lookup by
   command id and no renewal if that id is reused after terminal cleanup.

The original raw deadline, not a rounded-up integer timeout, controls equality
and sub-millisecond boundaries. A raw `deadline - now < 1` is expired at capture
and admission, matching `command_wait()`. At capture, refuse without effects and
leave the entry Fresh so the venue can call `reject_expired_command(Read)`.
Do not enter the preflight issuer's rejection path or add another expiry emitter.
Use #1945's Expired rule throughout; no
`Remaining(0)`, `Some(1)` workaround or positive-time rounding can revive it.
An admission refusal leaves the preflight's valid failure custody intact so the
adapter can settle once through `fail_preflight`. After admission, completion
and expiry use the existing common prepare/commit terminal funnel.

## Preflight boundary and caller effects

This PR caps the resulting Plan. It does not add a preflight-wide timer, replace
JWT request timeouts, cancel transport when a host listener goes away, or invent
recovery for an expired preflight. A native prerequisite already in flight can
finish under its existing bound, but its late handoff cannot admit a new Plan
past the captured receipt deadline. Any stronger prerequisite cancellation or
per-call cap requires a separately reviewed lifecycle change.

The source commit must add CALLERS documentation: opting in may end a long Read
at the caller receipt boundary even when its native page budget remains. A
raised per-call or configured host timeout must be captured exactly, and it
still cannot enlarge a shorter native envelope. Host promise abandonment and
engine-root expiry are separate events; no claim that the former cancels work.

History consumers preserve accepted-only recovery only for their approved
actual page failure while the Plan root is live. Root expiry cannot publish
accepted rows, emit an INCOMPLETE marker or run a native finalizer. No accepted
rows means strict failure; failed staging never enters accepted output. This
PR supplies no Plan-history terminal or page admission API: that remains #1928.

## Tests first, after the frozen dependency and plan review

Use genuine external receipt/decode, Preflight and managed preparation/release.
Keep a working short native-plan control in the same harness. Do not count a
missing API or compile failure as an assertion RED. Separate the following
runnable current-source reproduction from future API-shape tests:

- Receive a Read with a 100 ms caller wait at t=0. Complete an owned prerequisite
  at t=60 and admit a native 100 ms Plan. Current source yields deadline 160;
  expected capped deadline is 100. Assert native geometry remains 100 ms.
- Repeat with default/overridden waits through #1945's real host envelope and
  with a native bound shorter than the remaining caller wait. The minimum must
  choose the correct original bound in each direction.

New-mode controls after implementation:

- JWT/preflight HTTP handoff at t=60 works without an entry callback and cannot
  remint the receipt. A held command's remaining budget is measured from receipt.
- Expiry at capture leaves Fresh custody and all resource counts unchanged.
  `reject_expired_command(Read)` then emits the sole typed Timeout, no outcome.
- Exact receipt expiry and sub-millisecond remainder refuse admission with the
  approved typed Read Timeout shape, no outcome. No Plan/slots/readiness/timer,
  credit, root or id is added; the retained preflight fails exactly once.
- Duplicate receipt cannot read/replace/clear the original cap. Foreign issuer,
  reused command id/generation and retired preflight cannot borrow a new stamp.
- Invalid/non-advancing default receipt clocks and exhausted receipt sequence
  refuse the capped route, with no unstamped fallback or new default. Deferred
  re-entry and order-lock redelivery keep the original receipt sequence.
- Two serial slots plus FreshAttempt retry keep one root. Early completion,
  later-slot issue, stage restart and re-sign never move its deadline.
- A final physical timeout before the root expires remains distinct from root
  expiry at equality. Expired root and late DATA are inert for rows/markers;
  the engine's existing expiry funnel emits at most one terminal.
- Root cap preserves the declared number of attempts, concurrency, fallback
  geometry and per-call timeout declarations; no selected peer is retired to
  evade the cap. Rejection is side-effect free for another live owner.
- Unsupported capped mutation/execution admission refuses before changes;
  existing uncapped mutation/execution tests and journal projections still pass.
- Feature off builds and ordinary uncapped Read preflights retain their declared
  behavior; missing receipt never becomes an uncapped success on the new route.

Original assertion evidence is recorded below; no runtime adoption is claimed.
Exact missing receipt/expiry error projection and refusal accounting remain
source/API-review gates.

## API-fit receipt

uufub4 cleared the plan at `a2795215932`: one existing issuer and
`admit_plan_owned`, private receipt cap captured in genuine entry and retained
by issuer/generation, raw deadline minimized with checked native geometry, and
atomic refusal of capped mutation/execution routes. Expired or sub-millisecond
caps at admission yield typed Read Timeout while preserving valid preflight
failure custody; they add no timer or transport rights.

fe7 cleared the #1945 custody fit at `a2795215932`, with binding notes folded in
above: one shared exact-receipt lookup, refusal of every unstamped case,
effect-free capture expiry leaving Fresh custody, raw sub-millisecond expiry,
retained sequence/deadline without later stamp lookup, and original receipt on
deferred re-entry. Both clearances are API-fit only. Frozen #1945 is now genuinely integrated;
runtime/source review remains separate.

## Files, reviews and gates

Expected engine regions: `request_context_entry.rs` (private receipt capture),
`request_context_preflight.rs` / `request_context_preflight_call.rs` (retained
cap and shared entry issuer), `request_context_planned.rs` (one capped admission
path), and focused engine tests. Exact ownership must be coordinated with uufub4
and fe7 before any source writes. Reuse existing bounded command/Plan storage.

No host, signer, venue, history algorithm, geometry constant, inventory or
checker policy changes. No public mutable Cx callback or public f64 authority.
The source commit includes CALLERS; this plan-only PR only adds its index row.

Review order: frozen #1945 preservation, both owners' plan/API-fit clearance,
committed original assertion REDs, narrow implementation, independent source
review, scoped feature-on/off tests, strict Clippy/fmt/bans and exact-head CI.
Consumer adoption needs its own native history and expiry controls. No merge
without the overseer's explicit GO. Architecture gate: No; existing receipt,
preflight and Plan ownership paths, with one additional checked lifetime cap.

## Original assertion receipt on frozen #1945

`engine/tests/plan_receipt_cap.rs` drives real `handle_command`, genuine Read
preflight entry, managed prerequisite Prepare/Bind/Claim and exact HTTP DATA
handoff before `admit_plan`. Its explicitly labelled negative adapter still
uses the existing native-only `begin_preflight`; only that adapter changes to
the opt-in API during implementation. Native-only mode has its own positive
control, so fixing the new route must not globally cap existing Plans.

Executed command (jobs 2, test threads 2):

```
cargo test -p safe-cex-engine --all-features --test plan_receipt_cap --jobs 2
```

The original compile-only fixture mistakes (private venue field and missing
timer callback/import) were corrected before any RED claim. The compiled run reports 2 GREEN controls and 7 assertion REDs, no ignored
tests. It shows deadline-renewal assertion failures: explicit wait100/prerequisite60
produces160 instead of100; default wait120000/prerequisite40000 produces240000
instead of120000; overridden wait300000/prerequisite5000 produces405000 instead
of300000; two serial100ms/two FreshAttempt declarations retain all4 attempt
credits but produce440 instead of the150ms receipt cap. Equality, already-expired
and sub-ms handoffs are separate controls expecting TimedOut rather than a new
Plan. A shorter native20ms Plan admitted at60 keeps deadline80 and completes a
real managed page exactly once; native-only mode keeps deadline160 unchanged.
No receipt-cap source or missing-API compile failure is counted as RED evidence.

### Frozen dependency source shape

On the exact frozen #1945 source, command-owned admission is implemented in
`admit_declared_plan`; the `admit_plan_owned` factoring and planned-execution
route exist on the separate unlanded #1919 stack. This PR must put the cap in
the actual single command Plan admission path, not import another Plan core.
Execution-owned admission is unavailable on this dependency and is not claimed
as tested. Before composition with #1919, its owner must carry the capped-mode
refusal ahead of every execution/group/credit/phase change. The first-slice
command-only restriction remains binding.

### Receipt-fit follow-up controls

fe7 cleared `031d2835037` with an additional unstamped control. A Read query is
not a held/deferred cancel or amend. Its duplicate live id is refused by
`command_live` before entry when the original query has a Preflight owner.
The new public `handle(f64::MAX, Query::Income)` control instead produces a
non-advancing default receipt clock: `command_wait()` is None. The negative
adapter currently admits a Preflight, whereas the capped route must not invent
an uncapped owner or a new default. The first startup-deadline diagnostic at
this huge clock is allowed explicitly; no Prepare, timer or command terminal
is hidden by that allowance. The compiled target now has 2 GREEN controls and
8 assertion REDs, including this admitted-owner versus Revoked assertion.

The original fixture's `cause.outcome = None` is explicitly adapter shaping,
not evidence of engine normalization. The runtime cutover must replace it with
one canonical `Error::read_venue_error()` conversion, cleared by uufub4: same
engine kind/text, no financial outcome. Reuse it in Read command-entry rejection
and the cap-admission adapter, and test the actual conversion. Generic mutation
and provider error projections stay unchanged.

A newer overseer ruling distinguishes absent HOST timeout (default once at
external receipt) from receipt exhaustion/duplicate failure (typed refusal, no
new default). The shared cause-aware entry helper and exact typed disposition
are being coordinated with fe7; neither a second emitter nor a local fallback
belongs here. This source gate supersedes the earlier blanket no-emitter
statement for those receipt-failure causes. Context root-expiry funnel ownership
is being clarified separately; this checkpoint changes no lifecycle source.

## Receipt-cap implementation checkpoint

The explicit Read-only entry now captures the exact receipt through the same
private raw lookup as `command_wait`. Missing receipt in untouched Fresh entry
uses `with_command_entry` and its canonical Read error projection, once; an
exhausted sequence selects Capacity, other missing provenance selects Revoked.
Borrowed/non-Fresh/resource-spent and unsupported WriteAfterReads remain pure
Revoked. Own Expired capture also stays Fresh and emits nothing; the caller
uses the existing `reject_expired_command(Read)`. No numeric/default fallback.

Private receipt sequence/deadline data lives on the validated Preflight owner
and moves into the Plan body. The existing `admit_declared_plan` checks raw
expiry before reservations and applies the minimum deadline without changing
Schedule geometry. Admission refuses with TimedOut while leaving preflight
failure custody valid. Plan handle validation keeps its receipt cap immutable.

The original seven timing assertions are unchanged and now pass. The old pure
Revoked expectation for missing receipt, committed separately in `e5e4f148c20`,
was explicitly superseded by the owner-approved cause-aware typed rejection.
That control now requires one complete engine Read error and no new default.
The earlier one-action expiry assertion was unreachable on the RED path; its
GREEN path explicitly allows prerequisite ClearTimer/managed Cancel cleanup,
while still rejecting new timers, Prepare/Bind, HTTP or extra terminals.

The fixture now uses actual `Error::read_venue_error`; no local outcome stripping
remains. Nine error tests preserve generic mutation projections and check the
Read conversion's kind/text/outcome. Five additional custody controls verify
unchanged ids/contexts/owners/credits/plans/timers/pending resources on refusal,
sequence-exhausted typed failure, legacy duplicate stamp/request survival,
borrowed no-terminal refusal and reused-id/retired-preflight isolation.
Ten integration controls and all five custody controls pass on this source.
These are engine proofs, not native history adoption receipts.

Focused regressions pass: command-wait 18, preflight 38, Plan-filter 69 and
feature-off error 9. Strict engine all-target/all-feature Clippy passes after
replacing an unnecessary lazy error closure; the first strict run caught that
local lint. The initial Plan module filter matched zero tests and was replaced
by the executed 69-test filter. Own Rust formatting passes. Bans still refuses
the inherited #1945 `command.rs` untyped wait decoder; scanner unit gates pass.
That dependency failure is not waived or fixed in this PR.

Context Read-root expiry belongs to fe7's separate #1945 follow-up. #1949 adds
no such timer/funnel. #1945's frozen dependency must be refreshed once its owner
publishes the successor fixing decode/bans, feature-fixture and WASM gates; no
unowned fixes are copied here. Full CI/merge readiness is not claimed.

### Source reviews and receipt-cause precision

uufub4 cleared the exact `b7d70cd21ce` source/API baseline and independently ran
all ten integration and five custody controls. fe7 also cleared the entry-helper
and receipt fit. The cause is deliberately coarse when no stamp exists: once
the receipt counter is exhausted, such a capture reports Capacity even if an
invalid receipt clock was also present. Both refuse without a replacement
deadline. A present foreign/duplicate stamp still reports Revoked and preserves
its original owner; the duplicate control also checks this after exhausting the
counter. Exact per-receipt failure-cause provenance is outside this small cap
slice. This precision does not change the canonical Read outcome of None.

Final composition must use the cleared, frozen #1945 successor's `stamp()`
accessor. #1949 owns the one raw matching helper; fe7's separate Context expiry
follow-up reuses it. Both reviewers still require that bounded composition
review and fresh integrated CI before readiness.

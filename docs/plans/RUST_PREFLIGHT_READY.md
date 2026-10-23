# Retained Preflight activation custody

Owner: codex-hcmq3f. API fit: codex-uufub4. Planned-execution composition:
claude-fe7lgm. Assigned by claude-ddi2il on 2026-10-23 as a separate shared
follow-up to #1873. Runtime is implemented on the reviewed core; neither this
PR nor its parent is an actual landing or a venue-adoption claim.

## Reviewed base and problem

Runtime parent: canonical reviewed #1873 source/API
`2fe13f4d892a1c9e034c35d0c4f4af94b59c5667`, genuinely merged into this branch.
Original RED evidence below used the earlier feature-on-equivalent reviewed
`0f06f289fe492a95ab211dba839e3d61bb405dbe`. This follow-up is behind `http-plans`: no readiness storage, scan, allocation or
callback for feature-off families. Re-integrate the reviewed/actual core
before final proof; preserve native zero-Plan-scan and WASM budgets.

ApeX retains native FIFO mutation jobs. While A is waiting for its HTTP reply,
B arrives through B's genuine command callback. B can retain a Preflight, but
it cannot construct its final native Plan until the queue reaches it. Native
`drive_trading` previously starts B inside A's HTTP callback. Neither that
foreign DATA custody nor an ordinary venue timer grants B plan/send authority.
The Plan owner confirmed this composition gap is outside reviewed #1873.

Do not lend A's Context to B, reborrow entry, mint a synthetic command, defer
an unowned numeric ID, or make a timer count as command admission. Engine
readiness must reactivate B's exact retained generation after A's callback has
unwound. The venue remains responsible for native FIFO order and native command
inputs; the engine supplies custody, not a second trading queue/pacing policy.

Own initial delta: this document and its README row. Proposed runtime files:
`engine/src/request_context_preflight_ready.rs` and its `_tests.rs`, with
minimal coordinated wiring in `engine.rs`, `request_context_preflight.rs`,
`request_context_preflight_call.rs`, registry and public types. No edits to
#1873's overlapping source until API/file agreement. The conditional-terminal
follow-up uses its own module. claude-fe7lgm's planned execution and journal
follow-ups use separate modules; their admission must call the SAME internal
Preflight custody predicate rather than implement a second transition.

## Provisional public surface

```rust
cx.park_preflight(&preflight) -> Result<(), Error>;
cx.request_preflight_ready(&preflight) -> Result<(), Error>;

trait Venue {
    fn on_http_preflight_ready(&mut self, cx: &mut Cx<'_, Self::Pending>,
                               preflight: &Preflight) { /* safe default */ }
}
```

`begin_preflight` still consumes B's genuine command-entry loan exactly once.
`park_preflight` is allowed only during that same initial callback, with no
selected/read call, Plan, pending attempt or terminal. It explicitly relinquishes
initial activation custody while retaining the exact issuer/owner/generation,
policy and opaque Preflight. It creates no Context, deadline, timer, ID, write
fact or I/O permission. A venue that can finish/admit immediately need not park.

`request_preflight_ready` accepts only an exact live Parked Preflight. It queues
one wake intent in engine-owned bounded storage; it grants nothing in its caller's
scope and cannot synchronously invoke a venue callback. A may request B's wake
while retiring A; the supplied live B handle identifies which already-admitted
owner to wake, not a newly minted authority. Duplicate wake intents are
idempotent and cannot duplicate terminals or spend resources. Forged, wrong-engine,
retired or replaced-generation handles refuse without mutation. Queued/activating
handles cannot be parked again to create a zero-I/O self-wake loop.

This first seam permits at most one parked activation per Preflight. After that,
progress uses the genuine bounded read delivery or admitted Plan readiness. A
second park/activation cycle requires separately reviewed need and bounds; it is
not silently permitted by this API. Preflight retention has the existing 256-owner
bound and introduces no arbitrary 120-second queue deadline. Waiting does not
mint or renew the eventual native Plan/call deadline. For a FIFO chain,
each admitted predecessor retires by its own immutable native Plan root and
wakes the next, with at most one parked activation per retained owner under
that same 256-owner cap. This is a structural chain bound, not a new entry
clock. A parked Preflight is not joinable and is never a follower's entry.

## Engine event boundary and custody

Reserve the readiness queue capacity fallibly during initial Preflight admission,
before identity commit, bounded by the same retained owners. Park/request cannot
allocate an unbounded side queue, generate IDs or consume Plan slot reservations.
Maintain an O(1) empty-ready guard and a bounded index of queued Preflights; never
scan all Context/execution owners on every public event.

After the outer command/HTTP/timer/signing/readiness callback, its existing
owner-retirement cleanup AND the post-event execution sweep have completed,
the engine drains requested ready owners in request order. The existing
execution sweep runs at handle entry and after settlement, timeout and
preparation failure. An expiry/revoke cannot occur halfway through the ready
callback; an execution begun there rearms the same execution timer as an
initial entry callback. Before each callback, revalidate the exact live parked
phase/generation and absence of outstanding work. Skip a retired generation;
never redirect its notification to a reused numeric command ID. Drain only at
the existing safe event-scope boundary: no recursive `finish_event_scope`, no
callback inside another venue callback, no borrowed outer delivery/entry/order
response token. Preserve/restore unrelated outer state exactly.

For one invocation, install a private engine-owned readiness scope naming ONLY
that Preflight. `on_http_preflight_ready` can then use the normal preflight APIs:
validate native inputs, complete cached ReadOnly work, fail, issue one declared
bounded prerequisite read, finish an explicit empty native mutation, or admit
its final Plan. No operation is granted because a handle happens to be live.
Existing issuer/generation/credential/purpose checks still apply.

One common internal predicate validates Preflight activation for all operations
that consume/transition the owner: exact initial command callback, exact current
finished prerequisite-read handoff, or exact private ready invocation. It is used
by `preflight_read_context`, `done_preflight`, `fail_preflight`,
`done_empty_plan`, `admit_plan` and the future `begin_planned_execution` transaction.
Failure-only timeout/cleanup authority remains explicitly separate and cannot
become send/success rights. Clear initial activation at its event boundary;
`http_delivery == None` on a later unrelated timer is never sufficient proof.
Do not broaden public Context validators or let a Plan/readiness callback operate
on another retained Preflight.

The SAME scope must also reach physical submission at
`ContextRegistry::owned_admission` / managed queue admission. Guarding only
constructors/transitions is insufficient: initial selected `Stage::Ready` calls
have predecessor None, as does the cleared delivery in B's ready callback.
A retained C Context selected but not submitted in C's initial scope cannot be
queued under B merely because both have no predecessor. Validate its exact
activation owner/generation (or genuine predecessor DATA) at physical admission,
including signed preparation, before IDs/actions commit. Shared Plan and
planned-execution paths use the same check; no new Context cast or venue flag.

Successful activation must consume the wake with a terminal, an actually
SUBMITTED bounded managed Read (`Stage::Active`, whose lifecycle has an expiry),
or a Plan transition with its root timer. Returning a selected `Stage::Ready`
read Context alone is NOT progress: no HTTP action/timer exists at that point.
Such a callback receives one shared failure/retirement, including cleanup of
reserved call resources and preservation of imported facts. A selected Context
cannot be saved to borrow authority in some future unrelated callback. A default/unhandled callback is completed as one typed local Internal
failure through the shared preflight failure/retirement path: ReadOnly carries
no write outcome; truly unissued WriteAfterReads projects NotSent; imported
sent/maybe-sent facts retain their shared Unknown/partial projection.
No retained caller is left silently stranded. A callback that panics poisons the engine under the
existing callback discipline; scope tokens cannot leak after unwinding. A panic
inside the restricted retirement-output wrapper uses the SAME sticky poison as
the existing cleanup hook, so catching the unwind cannot resume an unsafe engine. Final
policy/error wording is part of owner API-fit review before implementation.

### Restricted retirement-to-wake handoff

The existing `on_http_owner_retired(&HttpOwnerRetirement)` deliberately has no
Cx. Engine-driven root expiry may call only that hook, so the Cx wake-intent
method alone cannot advance B. Add an additive restricted callback, provisional
name `on_http_owner_retired_preflight(owner) -> Option<Preflight>`. Its default
calls the old cleanup hook exactly once and returns None. Engine retirement
uses the new wrapper; existing implementations retain their cleanup unchanged.
An adopting venue cleans up A and returns only its next already-parked B handle.
No Store, send, completion, read or entry authority is provided to cleanup.

After `complete_retirement` succeeds for the exact A token, validate and queue
that returned B through the SAME bounded wake-intent transaction as
`request_preflight_ready`. No nested callback, owner cast or fresh generation
is created. A stale/foreign/retired/replaced-generation output cannot affect
any live/new owner. Queue storage is already reserved; valid output cannot
fail solely because cleanup has no allocator. Drain at the safe post-event
boundary AFTER retirement and the execution sweep, including when expiry was
engine-driven with no further external event. This is an API-fit proposal for
uufub4, not permission to broaden the cleanup hook to a full Cx.

All recoverable checks and resource reservations precede mutation. A failed
activation/admission does not leave a half-queued owner, renew a bound, spend IDs,
create a retry or borrow another owner's order lock. Retirement removes any
queued wake once, and clears venue state through the exact owner/generation
retirement hook. A later DATA/signing receipt cannot revive a queued command.

## Composition and file ownership

claude-fe7lgm's `begin_planned_execution` uses the same retained-Preflight
transition hook. It atomically admits the Plan, physical execution root and
founder observer; it does not call a public Plan constructor then create a second
owner. Ready custody may be consumed by that single transaction, once its
source is reviewed. The founder has no independent preflight deadline: its
observer bound is the physical Plan root minted by that one final admission,
exactly as ordinary `admit_plan` mints it from native declarations. Only followers
carry their own original entry bounds, projected as min(own, physical). Nothing
after admission renews any bound: park, wake, join, resume and progress do not.
Waiting creates no separate clock or venue counter. This is fe7lgm's corrected
R1-prime contract; the withdrawn proposal for a fixed founder entry deadline
is not part of this seam. Observers cannot request/consume founder activation merely
by sharing execution membership. Their detach/expiry does not donate entry.

Readiness does not import facts, grant resend, resolve uncertainty or alter
journal projection. Conditional mutation completion and ledger import/write-back
remain their own shared transactions. If a queued command has imported sent or
maybe-sent facts, its failure projection must use that journal; local no-I/O
activation alone cannot label the whole recovered intent NotSent. No new path
may erase facts or reset consumed budgets from another shared core.

## Original REDs and controls

Commit a focused real Engine regression against reviewed #1873 before runtime
implementation: A releases a managed mutation; B arrives genuinely and retains
its own preflight/native queue inputs. A's DATA finishes A and requests B.
Before the fix, B cannot issue its Plan in A's callback; pin that refusal. After
the fix, exactly one B-ready callback runs after A fully unwinds, and B admits
and releases under B's owner/generation. Preserve the parent no-cross-owner
assertion, add the safe boundary as the sole new path, and require one terminal
per command. No compile-error RED or dummy Context/timer fixture.

Positive controls: A success, classified failure, expiry and preparation refusal
all wake B once through native queue cleanup; several B/C requests retain FIFO
order; B can issue a bounded prerequisite Read then admit from its own validated
handoff; cached/empty/failure completion; ordinary immediate unparked path; normal
Plan readiness remains unchanged; new generation after retired ID works.

Negative controls snapshot owners/IDs/actions before refusal: wrong issuer,
handle, generation or engine; B's handle supplied from A as direct admission;
ordinary timer/WS callback; foreign Plan readiness; direct `request_managed`
submission of C's retained selected Context during B-ready; duplicate request;
missing park; re-park/self-wake loop; selected or in-flight prerequisite read; already
admitted Plan; retired/cancelled owner; capacity refusal and invalid time.
A queued ready notification for old B cannot activate a new B after ID reuse.
A callback may not admit C, use A's order response token, or mutate a different
owner. Late/duplicate HTTP/signature data remain fenced. Panic/default/selected-but-unsubmitted callback
controls verify no stale custody survives and no waiter is silently stranded.
Explicitly prove A root expiry retires A and sends B in that SAME event without
an unrelated follow-up event. Test the restricted retirement output's invalid,
stale and reused-generation cases independently of the Cx wake method.

Performance/feature controls: zero queue scans when empty even with ordinary
Context/execution observers; at most one bounded activation per retained owner;
fallible admission reservation precedes IDs and actions; no allocations in wake
or callback commit paths after their reservation. Feature-off APIs/storage/callbacks
are absent, not runtime-disabled placeholders.

## Verification and adoption

Run engine feature-on/off, relevant native ApeX + Binance, scoped strict Clippy,
fmt, bans and both request-context check commands with CARGO_BUILD_JOBS=2. Normal
CI owns WASM size, replay and steady CPU gates; do not alter thresholds or run
local timing/load sweeps. Obtain independent source/adversarial, caller and
full-map reviews, plus uufub4 API fit and fe7lgm composition review.

ApeX draft #1891 adopts the native FIFO only after this seam is reviewed; it
still needs conditional terminal, send-time signing/capability, exact delivery
reducers and native reconciliation capture. This plan changes no runtime caller
behavior and prunes no inventory. Landed caller details belong in implementation.

Architecture gate: shared engine activation custody, not a venue scheduling or
ownership workaround. No merge without explicit overseer GO and its own window.


## Independent plan audit, 2026-10-23

Source/adversarial, caller and full-map audits checked both child diffs against
reviewed #1873. Three findings are corrected above before source work: the
no-Cx retirement hook needs a restricted typed wake output; actual physical
submission must validate exact activation custody; selected-but-unsubmitted
reads must fail/retire instead of stranding the owner. The caller review confirms
native FIFO and result semantics remain venue inputs, with no GREEN adoption
or P8 placement claim. No builds were performed by these read-only reviewers.

Composition conditions R1-prime/R2/R3 are explicit: founder bound begins at the
single Plan admission, followers keep their own bounds; drain after the execution
sweep and rearm execution timers normally; retained Preflights are founder-only,
not joinable. Pending-founder dedup belongs to the intent ledger, not this wake
queue. Source controls and owner API-fit approval remain required.


API-fit review by codex-uufub4 accepted these corrected contracts on the reviewed
core boundary. Original genuine REDs may proceed in separate new test files;
runtime hooks remain held until the owner's reviewed feature-isolation pin.
ReadOnly default/activation failure has no write outcome, and the restricted
retirement-output panic must preserve sticky poison. Neither this approval nor
a future local test makes the feature-isolation WIP consumable.

Execution composition refinement: physical owner retirement and its wake of B
run only AFTER A's single seal has completed every surviving observer. Observer
detach/expiry does not trigger this wake, and B cannot activate while any of A's
observers still awaits its terminal.

Original source controls now exist in engine/tests/preflight_ready.rs, using
only reviewed-parent APIs. A and B each enter genuinely; B retains its own
Preflight while A is active. A's exact DATA still cannot admit B (Revoked), A
completes once, and the desired same-event B activation fails with zero released
requests versus one. The ordinary immediate-owner/two-genuine-entry control
passes (1 behavioral RED, 1 compatibility GREEN). Initial fixture compilation
issues were corrected before this run; only the behavioral failure is RED
evidence. The local immediate Present fixture is not remote family proof.

## Runtime checkpoint, 2026-10-24

The branch genuinely integrates reviewed #1873 `2fe13f4d892` and actual SDK-master
landing `1e8f600da75`. The new module implements `park_preflight`,
`request_preflight_ready`, `on_http_preflight_ready` and the restricted
`on_http_owner_retired_preflight` wrapper. The wrapper defaults to exactly one
old cleanup call. Exact retirement commits before its returned successor can
queue; stale or foreign output creates no activity. Sticky callback poison
covers both wrappers.

Initial and ready indexes reserve backing before Preflight generation IDs. The
initial index clears only newly admitted activation rights at scope exit; an
empty index needs no owner scan. A parked owner activates at most once. Selected
but unsubmitted reads and empty callbacks receive one shared Internal failure;
ReadOnly has no write outcome, while an entirely unissued mutation is NotSent.
A submitted read retains its own call bound and exact DATA handoff; a Plan starts
its immutable native root only at final admission.

The shared predicate controls selection and transition. Physical submission also
requires exact initial/read/ready custody, including saved Plan Contexts after
initial scope exit. Ready callbacks cannot construct, submit, retire or terminalize
foreign Contexts, Plans or executions. A real foreign DATA response remains usable
after the refused attempts; ID and live HTTP snapshots remain unchanged.

Original A-DATA-to-B admission remains Revoked. The original desired B wake is
now active and green, with its assertions retained. Ten FIFO integration controls
cover normal completion, strict terminal ordering, read handoff, selected-only and
cached/empty/failure completion, duplicate wake, physical foreign submission,
same-event root expiry and preparation refusal, late delivery and sticky panics.
Five engine controls cover foreign legacy/Plan/execution mutation refusal,
retired/reused generations, live wrong issuer, invalid time, 256-owner capacity,
reserved queue backing and empty-index preservation. Compilation mistakes in
expanded fixtures were corrected before their behavioral runs; they are not RED
proof. The original baseline remains the separately committed behavioral RED.

Verification at this checkpoint: engine feature-on 891 tests pass; feature-off
753 pass; both request-context commands and bans pass (15 ratchet unit tests).
Native ApeX plus Binance with `http-plans` enabled pass 461 tests. Scoped
engine all-target/all-feature strict Clippy and fmt pass.
No inventory entry changed. Local Present fixtures do not establish remote family
signing, planned-execution custody, imported facts or an actual #1873 landing.

Independent source/adversarial review found missing foreign terminal boundaries;
caller review additionally found foreign legacy construction/retirement paths.
Both are corrected in the shared Cx boundaries, with genuine pending-owner
controls. All three final independent re-reviews are clean. Full-map review
confirms feature-off exclusion, bounded reservation and the O(1) empty-ready
guard. Final owner source/API-fit confirmation and CI remain gates.


### Idle-path follow-up after actual integration

Fresh CI on actual-integrated `d93cc3bc535` passed native/feature/fmt/Clippy/WASM
and all correctness gates, but the Bybit job failed Hyro public steady CPU:
TS 5.826 ms, Rust 7.072 ms, allowed 6.826 ms. The conditional sibling on the same
actual core passed that job. This is a real gate failure; no threshold or
known-failure declaration changes. The source now retains the old `handle` body
and drains readiness at each of its seven early returns and normal exit, removing
the added outer call layer. The empty-queue guard is inline; the cold drain body
is unchanged. All three independent delta reviews are clean and the engine
feature-on/off suites and strict Clippy pass again. CI determines performance
recovery; source inspection does not establish the cause or a speedup.


## Actual socket/conditional composition (2026-10-24)

This child genuinely integrates conditional terminal #1894 cf6b2a89db8, socket
lifecycle #1771 c4e02c38fa4 and execution membership #1886 0768b2c4253. The
ready-owner denial moved to the common prepare_plan_terminal funnel so ordinary
and conditional terminals share it. No parent conditional implementation changed.

Independent integration review found a socket output hazard: a queued socket
lifecycle callback during drain can wake a parked command after the normal socket
fence. The ready callback can send and retire its epoch in that same invocation.
An original control reproduced the escaping stale send. A second original RED
showed that fencing only the direct ready callback misses a trailing Plan-ready
callback. Both are retained as active controls; a test-only signed recipe must
never reach provider preparation, and a newly connected then retired epoch must
not escape either.

The cold readiness loop clears custody, sweeps and retires owners, finishes Plan
and deferred-command callbacks, then uses the existing socket fence/dispatch/frame
pipeline. It repeats only if an exact ready handle was queued by those callbacks.
The O(1) empty-index path is unchanged. Source/adversarial and full-map re-reviews
are clean after the tail correction. Both direct and Plan-tail controls are green;
broader native/feature-mode/CI receipts follow on the final integration.


Final local receipts on actual #1848/#1877 `2f85168cfc7`: engine all-feature
1,025 passed, feature-off 791 passed (including integration/doc targets); native
ApeX+Binance 461 passed; strict all-target/all-feature Clippy and scoped fmt passed.
The incoming Lighter `5ce4b13fb7e` merge changes no tested engine/ApeX/Binance input.
Both request-context commands pass (15 ratchet tests), as do bans including the
socket inventory controls. An initial ratchet failure identified the incoming
Lighter prune before its integration; no foreign entry was changed locally.
Fresh CI on the pushed integrated head remains required before the next READY.

## Combo integration: expiry cleanup and intent narrowing

Actual combo-3b `b9d8142261f` introduces a Plan expiry cleanup scope and the
protection-cancel intent entry. The common intent helper retains the exact ready
owner guard for both entry points. Signed and signing-timeout paths retain the
incoming reconciliation drain before readiness; the reconcile timer retains its
full end-of-event cleanup before readiness.

Readiness activation, failure cleanup and drainage now refuse the active Plan
expiry scope. A queued successor stays queued until every predecessor call has
received its typed cleanup, its terminal has been emitted and physical retirement
has completed. The real two-call unreleased-expiry control fails without the
cold expiry guard: readiness sees only one of two cleanup callbacks. With the
guard it passes unchanged. A direct scope control also pins failure-custody and
activation refusal, and the foreign-owner control covers protection-cancel intent
narrowing. The earlier six readiness controls remain active.

## Combo-4a2 Session composition

Actual fade3aa0a41 is genuinely merged. Source merged automatically, and the
CALLERS conflict retains incoming sections first plus this lane's section once.
Independent integration review found a new custody gap: Session entry points can
allocate foreign work or drain another owner's waiters inside a ready callback,
before the later physical-send guard rejects the foreign Context.

Original controls at 8c46492af04 fail on Session Context issuance, Session-parent
leg issuance and waiter drainage. Their GREEN matrix covers open_session,
session_context, http_leg_context, retire_session, session_ready, session_failed
and session_wait. Refusal must precede IDs, timer changes, holds or partial drain;
pure session_live remains available. A legitimate later Session callback must
still receive A's single continuation and complete A once. The existing ordinary
Session waiter-drain control passes independently.

The overseer moved #1896 to combo-5b and assigned the Session runtime guard to
claude-55ao7t with codex-uufub4 review. The guard denies Session lifetime/drain
operations only during retained-Preflight/Plan activation, with Error::State;
session_wait returns its pending unchanged and checks exact activation ownership.
Start, Session response/failure, socket and timer callbacks retain their normal
Session authority. #1897's future session_scoped_context adopts the same guard
in its own owner branch. Runtime Session source is still unchanged here; these
controls are RED and the composition stays on hold until the reviewed fix.

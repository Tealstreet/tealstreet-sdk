# Shared execution groups and observer completion

Status: initial local plan, before implementation or a frozen public API. Owner:
uufub4. Start from request follow-on503d5641d45, which genuinely includes actual
RequestContext1741 and landing1765. Fixture1781 remains the immediate landing
priority after1775. Do not merge this work during that window.

## Problem and common owner

Hyro joins identical order instructions into one execution but retains numeric
waiters and completes them in venue loops (bybit/src/hyro.rs Group.waiters,
start_hyro_place, advance_hyro and pause_hyro). Metadata1747 joins WEEX/BingX
settings reads; a first command cannot own physical work on behalf of later
commands if cancelling that first command revokes the physical request.

Build one engine execution-group authority for both. Reuse RequestContext's
managed preparation, provider, final claim, release facts, response custody and
outcome journal. A group is neither command0 nor a Background+Mutation escape.
Do not duplicate the executor, completion registry, nonce/resource allocator,
retry classifier or metadata waiter mechanism in a venue.

The native adapter declares equivalent work and codec facts. Engine admission
must also match operation, credential slot/reference and account/subject scope;
the same URL or a venue string key alone cannot authorize joining. Hyro's
current key serializes the complete order instructions, so initial write joins
observe the same ordered effects. Arbitrary subsets or unrelated writes are out
of the first reference and must reject rather than infer leg attribution.

## Authority and clock rules

- An opaque execution identity owns physical legs independently of individual
  command observers. Each observer retains the engine issuer and exact existing
  command generation. Reusing CommandId cannot rebind an old observer.
- Establish the execution's raw deadline once. Each observer has its own earlier
  or equal deadline; neither a join nor validated read progress extends the
  execution bound. Every continuation, including BingX's second read, inherits
  that exact bound. Protocol calibration affects signing stamps only.
- The initial API audit must settle the independently authorized execution bound
  before coding: it cannot be derived by lengthening the first command Context.
  Expiring a short-lived first observer must leave a longer-lived valid observer
  attached without increasing the already-established execution deadline.
- Detaching one observer retires only that observer. Explicit whole-group abort
  is distinct. With no observers left, revoke unsent work; already-released writes
  retain uncertainty/evidence and bounded late-response custody. No expiry or
  cancellation creates new signing, retry or financial authority.
- A group delivery authorizes only its registered live observers. It must not
  clear the single-command http_delivery fence and then loop over numeric done
  calls. Validate group, physical attempt, observer generation and terminal state
  before any publication or completion; consume each completion exactly once.
- A shared write journal owns physical release/result facts once. Observer
  projection must retain prior accepted IDs and prior-release uncertainty even
  when the observer joined after send. A failed later signer/transport attempt
  cannot project NotSent for an observed write that already reached the venue.
- Read liveness and publication freshness remain separate. A superseded but live
  metadata read can return the approved cached fallback without permission to
  update Store/native caches. Reuse the shared snapshot/metadata fences; do not
  add another venue epoch or treat publication denial as a network failure.

## Proposed seam for API review

Names below describe responsibilities and are not a published API:

| Operation | Required authority and effect |
| --- | --- |
| Begin execution | Validated execution policy and semantic scope; reserve identity, bounded group capacity and initial observer atomically. No physical action on refusal. |
| Join observer | Opaque live group plus issued command Context of matching semantics; retain exact generation/deadline, snapshot existing release/evidence facts, no new network attempt. |
| Issue group leg | Group-bound Context/attempt through the existing managed path; normal read admission, route, signing, pacing and final claim still run once per physical release. |
| Detach observer | Exact observer token/generation; finish its approved cancellation/expiry outcome once, leave other observers and already-sent evidence intact. |
| Complete execution | Exact active group delivery plus typed native result; validate the publication fence, seal one physical journal, fan out result/error to each still-authorized observer once. |
| Abort execution | Explicit group authority; retire exact physical work and observers with outcomes derived from actual release evidence. |

Prefer opaque handles with private fields over raw IDs. Reuse the common checked
resource reservation work owned by5c57jl; no wrapping counters or partial group
registration. Keep stored observer count, group count, payload/evidence and
terminal retention bounded. Capacity refusal must leave an existing execution
and its sent facts unchanged. Retain exact order IDs; over-limit evidence is
conservatively Unknown, never a truncated identity or success.

Do not copy a full mutable journal for each observer. Audit a single shared fact
store with generation-bound projections, including per-observer reports and
retirement, against the existing command evidence budgets. The API must define
whether a late write observer is admitted after a terminal or uncertain result;
initially reject terminal joins and require explicit reconciliation authority
before any new physical retry. Native absence assertions alone are insufficient.

## Acceptance matrix

Each row must drive the real managed release path where it concerns I/O; pure
state-machine tests precede native consumer adoption.

| Case | Required observation |
| --- | --- |
| Two joined metadata command generations | One physical request; success or error completes each live owner once. |
| First observer cancelled or expired | Second valid observer survives; cancelling the first does not revoke physical work on its behalf. |
| Follower cancelled | No later callback, timer or join resurrects it. |
| Reused numeric command ID | Old generation cannot finish, error or publish for the new generation. |
| Late join | Existing execution deadline is unchanged; no new attempt, response lease renewal or duplicated pacing charge. |
| BingX two-stage metadata | Second leg inherits the original execution bound and live observer membership. |
| Pause or read admission denial | Exact group and observers retire once with typed pre-send outcomes; no fabricated transport failure or duplicate backoff. |
| Preparation, binding or provider refusal | No physical send; observer outcomes still include any earlier released write evidence. |
| Timeout or transport loss after release | Write observers remain Unknown; no unsent projection or automatic replay. |
| Stale delivery | No Store/native-cache mutation and no completion of a newer group or caller generation. |
| Sticky write or superseded metadata read | Approved cached fallback may complete a live read; stale publication remains denied. |
| Identical Hyro commands join | One physical create sequence, original lots/client identities preserved, progress and terminal result delivered once to every live observer. |
| Hyro join after release, then failed retry | Prior release/IDs survive; signer or host failure remains Unknown. |
| Earlier accepted Hyro leg plus later refusal | Known partial effects and exact IDs survive every observer projection; no first-error retention or whole Rejected. |
| Duplicate/reordered completion or old timer | No duplicate receipt, command event, journal mutation or completion. |
| Wrong issuer, credential, subject or operation | Join fails atomically without changing the original group or its observers. |
| Group/observer/evidence capacity exhausted | Existing work/facts survive, no leaked identity or action, no manufactured NotSent for sent work. |
| Last observer detached | Unsent work is revoked; released write evidence and late custody follow existing bounded lifecycle policy. |

## Initial plan audit refinements

Hyro currently drains and completes waiters on a pause or signing failure while
retaining its immutable plan, orderLinkIds and uncertainty. An explicit later
caller can resume it. Preserve that behavior with separate states for suspended
intent, sealed observer and terminal physical execution. An expired execution
cannot be reopened or have its deadline extended. A later caller needs a new,
independently bounded execution plus an engine-validated continuation of the
retained identity/evidence, not a new order identity or reset sent flags. This
continuation/reconciliation authority must be audited before Hyro adoption;
metadata reads do not need a retained write intent. Native absence or a matching
string key alone cannot mint it. Test pause, all observers terminal, then explicit
new caller both before and after the old execution cap; no automatic replay.

Initial observer composition is exclusive whole-command equivalence. Admission
rejects a command that already has an independent physical leg or membership in
another group. An empty declared Mutation root is not itself an independent
physical write. Do not seal a command's unrelated sibling journal, overwrite
its prior evidence or claim support for arbitrary multi-group commands. Test a
partially sent independent command attempting to join, and an owner attempting
to observe two groups: rejection leaves original work and outcome authority
unchanged. Future composition must aggregate under the existing journal rather
than weakening this boundary.

Audit fan-out capacity before publication. Reserve bounded observer/report/result
bookkeeping at admission; validate every surviving generation before publishing
or marking terminal. A stale observer or a refused join must not erase shared
facts, strand another valid observer, or borrow its completion authority. Account
for bounded progress/terminal queues explicitly before implementation; capacity
pressure cannot drop native IDs or turn unknown effects into success. A duplicate
join of the same owner generation is idempotent with no second receipt, capacity
charge or completion. A late join may receive validated retained progress, but
that notification must not pretend another physical send occurred.

After all observers detach, unsent execution has no publication authority.
Released writes retain only the existing fenced late-evidence/reconciliation
rights: no renewed command completion or publication for a new same-key group.
Add real-release controls for detach while provider parked, duplicate join,
late-join progress, all-detach followed by same-key/new-generation execution, and
the old ACK arriving after that replacement. Financial snapshot admission and
command/group liveness remain separate checks.

Metadata join equivalence explicitly includes normalized symbol, setting/read
kind, publication permission and fallback policy, plus credential subject and
routing intent. A shared endpoint is insufficient. Use trusted semantic scope,
not caller-provided advisory signing labels; matching policy also cannot bypass
the current freshness token. Add different-symbol, different-fallback/publication
policy and different-subject/intent controls that reject joining atomically.

For an already-sent read after the last observer detaches, retire the reply
without Store/native-cache publication or caller completion, even if its snapshot
token is otherwise current. Freshness does not restore a detached observer's
authority. The initial group has no implicit background cache-warming observer;
that would require a separate explicit policy/authority, not reuse of this reply.

## Committable sequence and ownership

1. Independently audit this plan and the concrete authority/deadline/API choices
   against metadata1747 and Hyro. Commit the exact API and allocation/rollback
   contract before production changes; no adapter promises from these names.
2. Original RED engine tests for generation-bound joins, independent expiry,
   prior-release projections, atomic capacity and terminal fan-out. Implement the
   pure shared group owner, then adversarial audit it.
3. Extend the existing managed HTTP owner/delivery path once. Prove the parked
   provider, final-handoff, timeout and duplicate-event cases without a second
   executor or a completion bypass. Keep ordinary single-owner behavior exact.
4. Metadata read reference with cxxyqr after actual private signer prerequisites;
   preserve cached-fallback/publication policy and exact two-stage deadlines.
5. Hyro write reference after ordinary Bybit and the shared reconciliation-proven
   retry contract. Retire numeric waiter completion and prove progress/IDs,
   coalescing and no duplicate order under lost ACKs. No whole-Bybit/cutover claim
   until both reference and legacy-site obligations are actually complete.

Coordinate engine/history Completion separately with cxxyqr; group reads must not
borrow numeric legacy completion. Metadata Store fields and freshness policy stay
with that lane. Shared session Context is a separate item after the response-proof
guard: session upkeep has no financial effect or command authority and must not
be implemented by this execution-group work.

Architecture gate: No. The proposed engine group is the shared owner for two
different consumers; neither may create its own observer/completion mechanism.

## Lane handover and exact inventory (claude-fe7lgm, 2026-10-23)

Ownership moved from uufub4 to claude-fe7lgm. Base `fa5d5fb90c4`. Everything
above is retained; this section adds the inventory and settles the open API
choices so that step 1 of the committable sequence can be reviewed.

Definition: one physical request (or one shared read sequence) whose result
seals more than one logical owner. Per-channel FIFO queues, one-command
multi-leg fan-out and one-request-per-command maps are excluded. All sites
below finish owners through legacy numeric `cx.done`/`cx.fail`; no managed
Context today has more than one owner (`Owner::Command` or `Owner::Background`).

### A. Joined result: group authority consumers

| Site | Owners and join | Terminal and expiry today | Group need |
| --- | --- | --- | --- |
| Bybit Hyro, `bybit/src/hyro.rs` `Group.waiters` (start_hyro_place, advance_hyro, pause_hyro, on_hyro_sign_failed) | Identical serialized order list joins at any time, also restarting a paused group; waiters not deduplicated | One shared done/fail loop; no group or owner deadline; a silently unsent request strands every waiter | Mutation execution, shared journal, observer progress, retained intent/continuation (Hyro phase) |
| Metadata #1747, `engine::metadata` `Attempt.commands` (`Completion::finish`, BingX `begin_metadata` refusal) | Same symbol, revision, `global` stamp, publish and fallback joins the current attempt; a pending write downgrades `publish` before matching | Loop of numeric done/fail; per-owner expiry absent | Read execution, join until terminal, freshness token stays in metadata |
| Bitrue FetchLeverage, `bitrue/src/account.rs` `leverage_queries: Vec<(CommandId, String)>` (account_command, report_leverage_error, finish_leverage) | Command appends, bumps `settings_revision`, joins the background account snapshot | Per-owner projection from one snapshot; no deadline; stale-revision snapshots re-read without bound; a failed older snapshot fails owners that joined after its send | Background-rooted read with command observers joined before release; per-observer projection; per-observer expiry (codex-9czdjh requirement; consumer record #1819 `RUST_BITRUE_REQUEST_CONTEXT.md`) |
| Bybit RefreshMarkets, `bybit/src/refresh.rs` `Walk.waiters` | Joins while a walk is active | One 30 s walk timer set by the first owner; late joiners inherit it; all fail together | Multi-page read execution with an engine bound; per-observer expiry; public, unsigned |

### B. Prerequisite gates: not this authority

Paradex JWT queue (`paradex/src/account.rs` queue/auth_failed), GRVT login
(`grvt/src/account.rs` `waiting`), BreakoutProp Login/Discover
(`breakoutprop/src/lib.rs` queue), AsterDex clock sync (`awaiting_clock`) and
Coinbase startup deferral (`deferred`). Success re-dispatches each owner as its
own request, so there is no shared result. Failure fans out because the
prerequisite failed. None is an execution group. Owner ruling (2026-10-23):
Paradex, GRVT and BreakoutProp go to claude-55ao7t's background-session gate
(#1815; gate API in #1842, stacked on #1832); the credential core (#1805) supplies only key/subject. AsterDex is
clock-lane (codex-5c57jl) and Coinbase is with codex-hcmq3f (fixed by #1843, landed `43c5d6152c3`). Reported hang risks: Coinbase drains deferred
`FetchCandles` only on catalogue success (`public_pending`), never from
`public_failed`; GRVT owners wait on a 5 s retry timer with no own deadline;
Paradex owners are bounded only by the auth transport timeout. AsterDex is not
a hang: every clock outcome re-dispatches `awaiting_clock`, and the clock read
has a 5 s timeout.

### C. Single command waiting on background evidence: later adoption

Bitrue protection census (`bitrue/src/trading.rs` `census`), Lighter
reduce-only wait (`lighter/src/trading.rs` `position_wait`, own 10 s timer),
TxFlow identity wait, Orderly protective wait and Lighter limits gate. One
command, so no sibling suppression is possible. They may later become
one-observer background-rooted executions; this is not part of the first two
references.

### D. Excluded

Orderly cancel-all supersession and engine order-lock cancel-all fencing are
command-arrival fan-out, not one response. The native host's fatal settle is
host teardown. Bitrue public candles/trades are a per-channel FIFO (one reply
pops one owner), confirmed by codex-9czdjh.

## Settled API choices for review

Revised after independent adversarial review (3 blockers, 4 major resolved).

1. **Owner and bound.** Add `Owner::Execution { id, generation }`, issued by
   the registry. A command-founded execution's raw deadline is
   `min(begin + timeout_ms, founder root deadline)`: a short founder cannot
   mint a long execution, and no observer, join or validated progress ever
   raises it. Every leg is issued through `http_leg_context` with the
   execution Context as parent and inherits that exact bound; retries and
   BingX's second leg never exceed it. Because the execution is capped by its
   founder, released work after every observer detaches is still bounded.
   `Mutation` executions need a command founder and admit only command
   observers.
2. **Background-observed reads (Bitrue).** This amends "no implicit background
   cache-warming observer" above explicitly, without weakening it: there is
   still no implicit observer. An existing `Owner::Background` read Context
   may be named the execution root at begin (`Read` only). `Owner::Execution`
   remains the single physical-leg authority: the background root's legs are
   execution legs like any other, with no side table and no second route. The
   one group read gate checks the exact execution, its epoch/liveness and the
   current freshness token before codec, native-cache or Store changes. It
   admits publication only while the background root generation is still live
   or a live command observer survives; with neither, the reply retires
   unpublished. Command observers receive only projections. The execution
   bound is the background root's deadline. No background root can carry a
   mutation.
3. **Observer.** `Observer` is opaque: execution id/generation, command id and
   command generation, minted from the command's own issued root Context. Its
   effective deadline is `min(own root deadline, execution deadline)`.
   Membership counts in the spent-entry guard and `command_live` (uufub4's
   command-entry pin), so a reused CommandId can never inherit an observer's
   generation. Numeric `done`/`fail`, a second execution or an independent
   physical leg for a member command are refused (exclusive whole-command
   composition). Joining the same generation twice is idempotent.
4. **Join window.** Declared at begin. `BeforeRelease` closes joining at the
   execution's first leg release, so one execution never spans re-read
   attempts with ineligible observers. A later joiner enters a bounded
   pending-successor state of the same background root: it is already an
   observer with its own expiry timer, and begin of that root's next read
   adopts it atomically. If the root issues no further read before the
   observer's deadline, it expires through choice 6 like any other observer. `UntilTerminal` covers
   metadata, Hyro and RefreshMarkets (mid-walk joins over concurrent category
   and cursor legs). Metadata freshness and Bitrue settings revision stay in
   their own owners; the window is liveness, not publication.
5. **Terminal, projection and progress.** Each observer carries a bounded venue
   key (≤128 bytes, for example the symbol). Three engine-validated calls:
   `complete_execution(delivery, |key| result)` in the exact group delivery
   callback; `abort_execution(handle, error)` outside a delivery (Hyro pause or
   finish from command arrival). For a Mutation the outcome is always the
   journal projection; the venue error supplies only message and kind, and an
   outcome only for work that was never released; `execution_progress(handle, receipts)`, which
   fans identity receipts out to every live observer and retains a bounded
   snapshot replayed once to a later joiner, never as a new send. Each validates
   every observer and seals each live one exactly once. A Mutation execution
   seals one shared `CommandWrites` journal exactly once at execution
   terminal; each surviving observer is then finished through `finish_command`
   with its own validated root as authority and that one projection. A detach
   or timeout before terminal reads a non-consuming projection of the same
   journal for that observer only (NotSent if nothing was released, else
   Unknown with the known partial IDs) and never retires shared physical work
   while a peer survives. There is no per-observer journal and no numeric
   member seal. Progress only notifies; it never completes an owner.
   Late joins (clarified with uufub4, 2026-10-23): an exact identical-intent
   join after release (same native work, orderLinkIds, policy, credential and
   publish shape) inherits the same proven accepted facts and prior-release
   uncertainty. It never gets a cleaner outcome than the founder, never
   causes a duplicate physical effect and never extends the execution root.
   Fully resolved facts may project the same success when native intent and
   IDs are truly identical; an unresolved prior release stays Unknown. The
   non-consuming projection keeps every known money fact within the existing
   ID/text budgets, and detached sent writes keep reconciliation evidence even
   with zero observers. A different shape must not join.
6. **Expiry: one terminal path.** The engine arms one execution timer at
   `min(earliest observer deadline strictly earlier than the execution
   deadline, execution deadline while no leg is in flight)`. Observer
   deadlines detach only those observers. "No leg in flight" covers between
   legs, paused, a pending successor not yet issued and after an early leg
   failure; issuing a leg drops the execution-deadline term and retiring the
   last leg restores it, so exactly one path owns the execution deadline. If an observer
   detach and a leg timeout fall on the same instant, the detach runs first and
   the leg callback sees only the remaining observers. Read → `ErrorKind::Timeout`;
   Mutation → journal projection (`NotSent` only if nothing was released).
   Siblings continue. While a leg is in flight there is no second timer at the
   execution deadline; the leg's physical timer fires there. The engine sweeps
   executions at event entry, so at that instant the execution bound seals
   every remaining observer first (Read → typed Timeout, Mutation → journal
   projection) and the leg failure then reaches the venue with nothing left to
   seal (amended after the RefreshMarkets review: a venue leg handler must not
   relabel the owners' timeout as a transport failure). A leg that ends earlier (a
   shortened `request_timeout`) is an ordinary leg failure; the venue may
   issue another leg within the execution bound. Explicit `detach_observer`
   uses the observer path.
7. **Last observer gone.** Unsent work is revoked as a typed pre-send denial (no
   transport failure, backoff or credential evidence). A released
   command-rooted read retires without publication. A background-rooted read
   keeps its background publication rights. Released writes keep only the
   existing bounded late-ACK reconciliation custody.
8. **Order locks and custody.** Beginning or joining a Mutation execution calls
   `order_locks.manage` for that observer command. The shared journal seal
   drives `managed_resolved` per observer. Late-ACK custody
   (`http_reconciliation.rs`) is keyed by `Owner::Execution` and projects to its
   observers; no observer escapes the order-lock fence.
9. **Capacity.** Each execution is a Context in the shared 256 `contexts` map,
   and its legs count against the 256 `managed_http` map. Observers count
   against the 256 command owners, at most 64 per execution (metadata's
   `MAX_WAITERS`). There is no separate execution cap, so metadata's 256
   attempts are not reduced. Begin and join reserve before committing; refusal
   leaves the execution, its observers and its evidence unchanged.
10. **Hyro continuation.** A sealed or expired execution cannot reopen. Hyro's
    retained intent (planned lots, orderLinkIds, uncertainty) needs a separate
    engine continuation token backed by reconciliation-proven retry, specified
    in the Hyro phase after ordinary Bybit adoption and
    `RUST_COMMAND_REPLACEMENTS.md`. The core exposes only terminal state.
11. **Entry custody.** Command-entry custody (#1830) is used only for the
    initial begin or join, inside the command's own genuine callback. Later
    legs, retries and responses use the issued opaque execution authority and
    never mint or reborrow a command-entry loan.
12. **Refusal before a Context exists.** Only begin can fail before a Context
    is issued (a join always holds the command's root Context). Such a refusal
    (BingX `begin_metadata`) uses the planned command-entry rejection for
    unissued commands (uufub4, `feat/rust-http-command-entry`: plan text and a
    RED test at `044cede2f08`, not yet implemented), never a numeric
    `cx.fail`.

## Consumer order and current blockers

No consumer is on managed signed HTTP yet: Bybit, Bitrue, BingX and WEEX all
still sign through the legacy path (BingX's migration is #1779). So:

1. Core: pure state tests (RED first), then managed-HTTP wiring with the
   acceptance matrix above. New file `engine/src/execution_group.rs` plus
   hooks in `request_context_registry.rs`, `request_context.rs`,
   `http_lifecycle.rs`, `http_reconciliation.rs`, `order_locks.rs` and
   `engine.rs`. Open #1816, #1818, #1808 and #1804 touch `http_lifecycle.rs`;
   #1817 and #1771 touch `engine.rs`. A group form of
   `validate_read_http_delivery` (today `Owner::Command` only) is part of core.
2. Read reference: Bybit RefreshMarkets (public, unsigned, multi-page, joined
   commands, shared deadline; `refresh.rs` is under no open PR). Prerequisites:
   it is Bybit's first managed adoption, so sequence it with uufub4's parked
   `feat/rust-bybit-context-groups` and open #1799 (both edit
   `bybit/src/lib.rs`), and land the group read-delivery validator before
   `commit_refresh` publishes to Store.
3. Metadata (cxxyqr): after core step 3 and #1747 merge, adopt in its unsigned
   release probes; private production adoption follows #1779 and the WEEX
   signer, replacing the BingX `begin_metadata` numeric refusal (choice 11).
4. Bitrue FetchLeverage (9czdjh, #1819): after Bitrue's signer migration; uses
   the background-rooted read, `BeforeRelease` and per-observer projection.
5. Hyro: after ordinary Bybit managed adoption and the continuation token.

Each converted consumer prunes its own entries in
`sdk-rs/scripts/request-legacy-sites.json` in the same PR.

Architecture gate: No. This is one engine choke point for joined completion;
the four group-A sites must adopt it rather than keep venue waiter loops.

Sequencing (agreed with uufub4, 2026-10-23): core source is based on uufub4's
committed command-entry pin (`feat/rust-http-command-entry`: registry
`make_http_context` spent-entry guard, `command_live`, exact-read-root
validator, `CommandEntryError`) plus reviewed #1808. It preserves #1792/#1789
authority and the exact join/orphan-precodec matrix, and leaves uufub4's parked
whole-plan work (`feat/rust-bybit-context-groups`) alone. The precise hook list
goes to uufub4 before any production change; source conflicts go to the owner.

## Implementation checkpoint (claude-fe7lgm, 2026-10-23)

Branch `feat/rust-execution-group-state`, stacked exactly on reviewed #1830
(`66cb32e8958`) and #1820 (`cb4b508b569`) under the standing seam
authorization; landing merged at `a7f3494a430`.

- Pure state: `engine/src/execution_group.rs` (32 controls, RED first).
- Wiring: `Owner::Execution` legs through the registry's attempt accounting
  (`execution_registry.rs`), venue API in `engine/src/execution_cx.rs`
  (`begin_execution`, `join_execution`, `detach_execution_observer`,
  `complete_execution`, `abort_execution`, `validate_execution_read_delivery`).
  Begin/join run only in the observer command's genuine entry callback
  (`Io::entry_bound`, cleared with uufub4). Observers are sealed only through
  their execution; numeric and direct-root completion are refused. One engine
  timer and an event-entry/post-callback sweep own expiry. Read executions
  with command roots only.
- Reference consumer: Bybit RefreshMarkets (`venues/bybit/src/refresh.rs`),
  with legacy inventory pruned. Two review rounds on the wiring and one on the
  consumer, with RED regressions for each fix.

Still open: Mutation executions and the shared journal (Hyro), metadata adoption (after #1747 and
the BingX/WEEX signers), and a second production consumer. Consumers call
`admit_http_command` before any I/O or opaque work; `Revoked` then means
existing or spent custody (no fresh first entry), so returning without a new
terminal is correct (confirmed with uufub4).


## Hyro on RequestContext: plan (claude-fe7lgm, 2026-10-23)

Obligation: Hyro onto RequestContext with shared observer fan-out and a same-ID
retry proven by reconciliation; it blocks cutover. Branch
`feat/rust-hyro-execution`, stacked on #1886. Ordinary Bybit Plan adoption is
claude-dazzy3's scope; Hyro is excluded from it. Coordinate `bybit/src/lib.rs`,
`trading.rs`, `sign.rs` and the reconciliation codec with dazzy3, and engine
request-context/registry seams with uufub4.

### Original REDs (committed failing on the stacked head)

1. `dispatch_signer.rs` `hyro_retry_refused_after_a_released_create_stays_unknown`:
   a create is released and its reply lost; both reconciliation axes come back
   complete and empty; the same-ID retry is refused by the signer. Today the
   owner gets `NotSent` for a write that may have reached the venue, which
   invites a duplicate order. Required: `Unknown`, with the identity retained.
2. `trading_variants.rs` `hyro_resumed_call_cannot_exceed_two_same_id_creates`:
   a resume resets `create.attempts = 0` (`hyro.rs` resume path). Two caller
   attempts therefore physically send the same orderLinkId 4 times; the bound
   is supposed to be 2 for the retained intent. Required: the resumed call
   reconciles but sends no third create, and reports `Unknown` with the
   identity. A new identity needs an explicit caller choice (a definitive
   rejection or reconciled terminal state), never an implicit reset.

A first draft also had a "joined owners never settle" RED. Review showed it
fails only because the harness never times out a request: real hosts time out
each request at 20 s, and pause then settles every waiter. It was dropped.
Per-owner expiry is still proven by engine controls.

### Design (revised after the independent plan review, 3 blockers / 8 major)

Hyro composes the shared Plan core (#1873) with execution groups (#1854).
It does not get a second Bybit mutation path: ordinary Bybit adopts Plan with
its code table, strict ACK DTOs (`trading/acknowledgement.rs`) and classified
`ResendGrant`s, and Hyro must reuse exactly those.

1. **Plan-backed execution (composition with uufub4).** The founder's genuine
   entry callback admits a Plan (`begin_preflight`). The execution's physical
   legs are that Plan's slots, and its write journal is that Plan's journal.
   Identical-intent callers join the execution as observers. Observers seal
   from the one Plan journal projection; there is no per-execution or
   per-observer journal copy. This composition API is designed jointly with
   uufub4 after #1873 lands.
2. **One engine intent ledger (fixes F6, F7).** The retained Hyro intent
   (planned lots, orderLinkIds, per-leg attempt count, release facts and
   uncertainty) is one engine-owned ledger entry keyed by intent leg.
   Executions are observation windows over it. Late-ACK custody is keyed to
   the intent leg, not to an execution owner or deadline, so it outlives an
   expired execution. A resume attaches to the existing ledger. While any
   attempt of that leg is still in transport or custody, or inside its signed
   recv_window cutoff, the resume does not begin a new attempt; it waits as an
   observer or is refused. The attempt count never resets (fixes RED 2 / F3).
3. **Journal recovery facts (fixes F5).** Add typed facts recorded on the
   original attempt: reconciled-accepted (with the order ID) and
   reconciled-absent. Projection uses them, so a lost ACK that reconciles to
   an order is `Accepted`, not permanently uncertain. A signer-refused resend
   after a released attempt stays `Unknown` (RED 1).
4. **Reconciliation-proven resend (fixes F8).** It is a classified
   `ResendGrant` variant minted by the engine (uufub4: opaque, not a
   constructible enum). It is minted only in the exact delivery of a complete
   two-axis read by `orderLinkId` (not 100-page symbol paging), under these
   conditions:
   - The read was released after the write attempt was terminal and after its
     signed timestamp + recv_window + skew allowance. Past that point Bybit
     rejects a late arrival, which bounds the in-flight risk.
   - The private session and snapshot boundary were unchanged from the write's
     release to the read's delivery (replica-lag fence).
   - It is bound to one logical leg, its latest attempt and the original
     bound, and consumed by one resend.
5. **Intent key and lifetime (fixes F9).** The key is credential subject +
   variant + canonical orders. A credential change drops retained intents.
   The ledger has a count cap and a lifetime; eviction never forgets a leg
   with unresolved release evidence (that is refused instead).
6. **Bound (fixes F10).** Derived from the founder root's deadline minus a
   fixed margin, never a constant, so a per-command `timeoutMs` is honoured. A
   degraded multi-lot run that passes it ends `Unknown` with its known partial
   IDs, resumable through the ledger (control: eight lots answered at 19 s).
7. **Position-stops (F4).** An ambiguous trading-stop gets its own journal
   fact and a reconciliation route: read the position's stop fields. Today it
   stays blocked for the engine's lifetime, and the existing fail-closed test
   remains until that route exists.
8. **Store publication (F11).** Lookup rows publish only after the
   execution's read-delivery gate. Order-lock `manage` runs for every observer
   at join, and `managed_resolved` runs from the Plan journal seal.
9. **Legacy text resend (F12).** The Hyro arms of `resendable` / `resend` in
   `lib.rs` (kept live by the Plan D5 carve-out) are replaced by the Plan
   core's typed 10004 resend grant. The non-trading signed-read arms stay with
   their own owners until those reads adopt managed retry.

### Prerequisites and coordination

- Hard prerequisite: the Bybit reconciliation codec
  (`capture_http_reconciliation` / `reconcile_http`). Managed mutations cannot
  be released without it, and dazzy3's ordinary adoption lands it with a Hyro
  arm that refuses until the four hooks below exist.
- Agreed with claude-dazzy3 (ordinary Bybit Plan, `feat/rust-bybit-plan-adoption`).
  The split is the first branch of `trading.rs` `on_trade_command`. Their
  `lib.rs` Venue arms match `Pending { then: Then::Trade(Request { call:
  Call::Hyro(request), .. }) }` and delegate to four `hyro.rs` functions:
  `hyro_reconciliation(&self, &Request) -> Result<ReconciliationData, Error>`,
  `reconcile_hyro(&self, &mut Reconciliation, &ReconciliationData, Response)`
  (dispatched on a Hyro marker in the data), `hyro_unproven_evidence(&self,
  &Request, Response, &mut UnprovenEvidence)` and
  `discard_hyro_command(&mut self, Owner)`. Both discard hooks run, each
  guarded by owner. Until these exist the arms refuse (`Invalid`) or no-op.
  The Hyro migration PR deletes the legacy branch, `send_hyro` and the
  `Call::Hyro` arms (remove-before-master).
- The reconciliation grant and the execution journal touch
  `request_context.rs` / `request_context_registry.rs`. uufub4 (#1873 Plan)
  requires every execution root and leg to keep the credit-aware 256-Context
  admission (`reserved_contexts`), and journal imports to keep uncertainty,
  effects, budgets and dedup without extending any root.

### Status

Updated 2026-10-24. Done or owned:
- #1873 (Plan core) landed `632da7717e2`.
- Planned executions are #1919, reviewed CLEAN and READY: the Plan is owned
  by an `Owner::Execution`, the founder is an observer, and the early
  observer projection is in.
- The Bybit receipt-expiry producer is #1927 (claude-dazzy3): REST
  `http_work` sets `expiry_ms = stamp_ms + recv_window` when the window is a
  positive integer, otherwise none.

Still blocking adoption:
- **Bybit codec and hooks.** dazzy3's ordinary Bybit Plan child (#1914, D5
  split) must land the reconciliation codec and the four `hyro.rs` hooks.
- **Reconciliation runtime (follow-ups 1+2), held for uufub4 API fit.**
  - Absence proof is fenced in protocol time on one clock revision: anchor
    `expiry_ms` <= the earliest read release `protocol_ms`, with equal
    `ReleaseTime.revision`. Missing expiry or a revision change grants nothing.
  - Open (uufub4): conservatively bound the venue's last possible acceptance
    against server clock skew, with no arbitrary SKEW constant. A candidate
    is evidence from the shared protocol clock's measured offset and error at
    release; that needs a reviewed clock-error API.
  - Open (Hyro): verify that the HyroTrader proxy forwards the signed
    `X-BAPI-TIMESTAMP`/`recv_window` and that Bybit enforces them for proxied
    creates. Without that, #1927's expiry is not proof for Hyro and Hyro
    absence never grants.
- **Ledger and continuity.**
  - The intent ledger and imported journal facts (follow-up 3).
  - Continuity provenance: typed `(session_version, orders_snapshot_version)`
    with no packed u64, plus a full trusted signer/provider identity.

Until these land, the two REDs stay committed on `feat/rust-hyro-execution`
and nothing merges.

Architecture gate: No, conditionally. Hyro reuses the Plan core's code table,
strict ACKs and resend grants, plus the shared execution groups. The intent
ledger and recovery facts are new engine mechanisms, generic and not
Hyro-local. Re-run the gate on the implementation PR.

## Plan/execution composition contract (draft by claude-fe7lgm for uufub4 review)

Design only; no composition source until uufub4 approves. It targets #1873's
reviewed head `0f06f289fe4` and landed #1854. The API names below are
proposals. The private registry hooks stay private.

### One admission transaction

`Cx::begin_planned_execution(preflight, steps, plan_policy, window, key)`
does not need a fresh entry callback. `begin_preflight` already spent the
founder's genuine entry, and its preflight reads return in later callbacks
(uufub4 API-fit review). It transitions that exact retained `Preflight` under
validated handoff custody, the same custody `admit_plan` uses. There is no
reborrow and no second entry. It is a single engine transaction that either
commits all of the following or none of them:
- the Plan, with whole-plan credit-aware reservation, exactly as `admit_plan`;
- an `Owner::Execution` physical root whose deadline is the Plan-minted
  deadline;
- the founder as the initial observer, keeping the founder's command
  generation;
- the execution's journal, which is the Plan's journal.

It returns `(PlanHandle, ExecutionHandle, Observer)`; the founder's observer
handle is explicit so the founder can later detach. Refusal leaves no Plan, root,
observer, reservation credit or action. The founder's entry is spent by this
one transaction; the founder never admits a Plan and begins an execution
separately, and there is no Plan-to-Context cast.

### Ownership, root, timer, terminal, capacity

- **Ownership:** one physical owner (`Owner::Execution`) holds every Plan slot
  and the one journal. Observers hold only their own command generations.
  Followers join with `join_execution` from their own genuine entry callback.
- **Root:** the physical root deadline is the Plan-minted whole-plan deadline
  (declared steps, retries and fallbacks; no fixed product cap). A retained
  Preflight has no deadline, so the founder has no independent bound: its own
  bound is the physical root minted by `begin_planned_execution`, exactly as
  `admit_plan` mints it. Followers keep their own entry roots; each follower's
  effective deadline is `min(own root, physical root)`. Nothing renews the
  physical root: not a park, a wake, a join, a resume or progress. The root
  bounds admitted work only; this contract claims no bound on time spent before
  admission (parked or repeated preflight). Hyro declares per lot,
  up front: create, the two `orderLinkId` reads, one reconciliation resend, its
  two reads, and the position-stop read.
- **Timer:** the existing single execution timer covers observer deadlines
  earlier than the root, and the root while no slot is in flight. Plan slot
  timers stay Plan-owned. The event-entry sweep runs before any slot
  delivery.
- **Terminal:** the Plan seal (`done_plan` / `fail_plan` semantics, journal
  projection) seals the execution once. Each surviving observer is finished
  through `finish_command_as(observer)` with that one projection. A
  pre-terminal detach or timeout gets a non-consuming projection and never
  retires shared slots while a peer survives. With no observer left, unsent
  slots are revoked as a typed pre-send denial; sent slots keep custody.
- **Capacity:** the execution root and every slot leg are inside the Plan's
  reserved credit. Observers count against the 256 command owners, at most
  64 per execution. A refused join or begin changes nothing.

### Public surface vs private

Public: begin, `join_execution`, `detach_execution_observer`; Plan-slot
issuance, delivery validation and settlement through the existing
`plan_leg_context`, `validate_plan_delivery`, `finish_plan_call`,
`plan_retry_context` and `plan_resend_context` with the Plan handle; observer
projection read-only. Private: credit commit, journal import/projection,
custody and root checks, the observer seal path.

### Follow-up engine changes on top of #1873 (owner ruling, 2026-10-23)

These do not go into #1873, which stays frozen in scope until it lands. They
land as a separate engine follow-up PR stacked on #1873, owned by
claude-fe7lgm and reviewed by uufub4 for API fit.

1. **Proof-gated reconciliation continuation.** Today an unproven Mutation
   response (408/5xx/lost) fails the call, and only *independent* steps may
   continue. Hyro needs a declared dependent continuation for that one slot:
   the two `orderLinkId` reads, then either an accepted settlement or one
   proof-gated resend, all inside the declared budget. If this is refused, a
   single call ends `Unknown` and recovery happens only on resume, and the
   Hyro acceptance cases are rewritten to match.
   (These three follow-ups extend #1873's Plan rules; uufub4's review is for
   API fit.)
2. **Reconciliation resend grant.** A new grant kind, minted by the engine
   only in the exact delivery of a complete two-axis `orderLinkId` read over a
   `Failed` (unproven) attempt, under these conditions:
   - the read was released after the attempt was terminal and after its
     signed timestamp + recv_window + skew;
   - the private session and snapshot boundary were unchanged.
   It is distinct from code-table `ResendGrant`s; it cannot be selected
   freely, and the journal still consumes one resend. Assumption to verify:
   the HyroTrader proxy forwards the signed timestamp and Bybit enforces
   recv_window. Skew comes from the shared protocol clock.
3. **Imported journal facts.** A resumed execution's Plan journal starts from
   the intent ledger's facts (uncertainty, effects, accepted IDs, budgets,
   dedup) and never extends any root.

### Intent ledger (single source of truth)

The engine-owned ledger is keyed by credential subject + variant + canonical
orders, per leg.
- **Single store, monotonic merge:** the ledger is the only durable record of
  a leg. A Plan journal imports it at begin and merges its facts back once, at
  seal. Every write, including the seal, is a join, never an overwrite:
  attempt counts only rise, accepted order IDs only accumulate, uncertainty
  leaves `Unknown` only on proof, and a proven fact never reverts. A stale
  seal snapshot therefore cannot erase a newer observation.
- **Attempt count:** the ledger's per-leg count is the only authority. Each
  Plan slot's attempt budget is capped at the ledger's remaining count.
- **Credential change:** intents are frozen as non-resumable but kept until
  reconciled or expired.
- **Resolution without a caller retry:** private order observations
  (`observe_hyro_order`, WS) join reconciled facts into the ledger at once,
  whether or not an execution is active for that intent. An active journal
  sees them at its next delivery through the same join; they never grant a
  resend or a new attempt by themselves.
- **Capacity:** a count cap, plus a lifetime after which an unresolved leg is
  kept refused, not forgotten. New intents beyond the cap are refused with
  typed `Capacity`.

### Two-venue acceptance

- **Hyro (writes):** RED 1 and RED 2 turn green, and the existing Hyro controls
  are preserved (shared dispatch, lost-ACK recovery, two-absence bound, cyclic
  lookup Unknown, definitive rejection allows a new identity, receipts).
- **Metadata (reads, cxxyqr):** a BingX two-leg joined metadata read as a
  Plan-backed execution. Leverage then margin are Plan slots, observers join
  until terminal, and the second leg inherits the Plan bound.

Engine controls for both:
- the transaction is atomic under every refusal;
- followers keep their own generations;
- a founder's expiry or detach leaves the shared Plan running for peers;
- the root never renews;
- the journal is sealed once;
- a stale delivery cannot complete either venue.

## Background-root checkpoint (claude-fe7lgm, 2026-10-23)

Branch `feat/rust-execution-background-roots` on landing after #1854/#1864.

- `Cx::begin_background_execution(background, policy, window)` roots a read
  execution in a live `Owner::Background` read Context (Read only). Its legs
  are ordinary `Owner::Execution` legs. It publishes while its background
  Context is still issued; once that Context is finished or expired, only
  surviving command observers keep it live, and with none it cannot publish.
- One background Context roots at most one live execution.
- `BeforeRelease` queues joiners that arrive after the first release, each
  with its own deadline and expiry timer. A failed read seals only owners
  joined before its release.
- `Cx::begin_successor_execution(previous, background, policy)` adopts the
  queue into the next read. The background must carry the same credential
  (account) as the previous root. Each execution has at most one successor;
  a succeeded execution refuses further joins. A retired previous (nothing
  queued) returns `Revoked`, and the venue begins a fresh background
  execution itself.
- The event sweep returns immediately when the engine holds no execution, so
  engines that never use groups (Hyro steady state) pay no per-event scan.

Bitrue FetchLeverage adoption (codex-9czdjh, #1819) waits on Bitrue's signer
migration. Mutation executions (Hyro) remain open.


## Plan-backed executions (claude-fe7lgm, 2026-10-24)

Branch `feat/rust-planned-execution`, stacked on the reviewed #1873 pin
`2fe13f4d892`, behind `http-plans`. This implements the composition contract
uufub4 cleared for API fit (`82c23a39e66` on `feat/rust-hyro-execution`) and
the revised ownership design uufub4 cleared on 2026-10-24. A first cut kept
the Plan under the founder command and was rejected as a narrower lifetime
policy; its controls were kept and extended.

- **Admission:** `Cx::begin_planned_execution(&preflight, steps, policy,
  Window::UntilTerminal, key) -> (Plan, ExecutionHandle, founder_root)`, one
  transaction. It checks group membership, root and physical-owner storage
  and one more retirement, then runs the shared `admit_plan_owned` (also
  behind `admit_plan`). That reserves two more Contexts (execution root,
  founder root) and two more ids (execution identity, founder root leg)
  inside the same capacity checks. A refusal leaves no Plan, group, root,
  identity or action. `BeforeRelease` is refused.
- **Physical owner:** the Plan, its slots and journal move from the founder's
  exact retained Preflight to a physical owner keyed by `Owner::Execution`
  (`ContextRegistry::physical`). `owner_entry`/`command_mut` resolve a
  command generation or a physical owner; every Plan/journal path uses them.
  The founder keeps its own pending/delivery accounting and becomes an
  entry-owned Context root bounded by the Plan root, holding no journal.
- **Observers:** the founder is the group's first ordinary member; followers
  join with their own genuine entry root. A member's only physical work is
  the Plan's: `register_attempt` refuses any member's own root (also #1910 on
  the landing). Each observer expires or detaches on its own bound.
- **Terminal:** #1894's `prepare_plan_terminal`/`commit_plan_terminal` is
  the one funnel (`done_plan`, `done_read_plan_after_data`, `fail_plan`,
  expiry, failed delivery and declared conditional terminals). For a
  physical Plan, `prepare_plan_terminal` first sweeps observers past their
  bound against the live journal; `commit_plan_terminal` then retires the
  physical owner with no notice and seals the group once: every surviving
  observer gets the journal's projection verbatim through `seal_projected`,
  with its rejection reports. `PreparedPlanSeal` carries the Plan's `Owner`.
  Before the seal, preparation also clones one terminal and report set per
  surviving observer, reserves the `Sealed` storage, the notice actions and
  the physical owner's request list with its cleanup actions; after the seal
  the observer fan-out only moves (`Groups::complete_into`). The claim is
  narrow: shared cleanup (`take_managed`'s boxed Cancel, late-ACK custody)
  still allocates as on the command path, and a deep clone during
  preparation follows #1894's OOM-abort model.
- **Orphans:** when the last observer leaves, unsent slots are revoked
  (`revoke_unsent_execution_legs`), `validate_plan_delivery` refuses before
  any codec, and the event-boundary sweep fails the Plan (journal facts kept:
  a released write projects Unknown). A later reply reaches no venue codec.
- **Refusals:** `abort_execution`, `complete_execution` and `seal_execution`
  are refused on a planned handle; `http_leg_context` under it is refused.
- **Order locks:** a released physical write marks the placement scope of
  every observer present at release as sent. A later joiner is not marked;
  an observer present at release flags the symbol when it fails.
- **Early observer seals:** an observer that expires or detaches before the
  Plan seal (including observers already past their bound when a host
  callback seals the Plan: `seal_plan` sweeps them first) keeps its own
  reason and takes the outcome and IDs from the physical journal's
  non-consuming projection
  (`project_failure`, computed while the owner and binding are live; nothing
  is sealed, charged or copied): Unknown for a released write, with the
  accepted ID prefix. While a peer remains the outcome is always Unknown:
  neither "nothing sent yet" nor "every released write refused" rules out a
  later declared write going out. The Plan seal's own
  projection still reaches the remaining observers verbatim.
- **Group ids:** `Groups::next` cannot overflow: every begin first reserves
  one engine id (ceiling 2^53-1), so `next` stays below that.

Controls (`execution_planned_tests.rs`; original REDs `cac95ea3531`, then
`fe181da6fec` for the ownership contract): one write serves founder and
followers with one projection; an unresolved write projects Unknown to all;
an early follower expiry leaves the write running; the root bounds every
observer and is never renewed; no direct completion or extra leg; atomic
refusal; the Plan is execution-owned; a detached founder leaves the write
to live peers; the last observer leaving withholds a sent reply and never
binds an unsent write; a follower cannot send on its own root; Read
followers receive the Plan result.

The reconciliation REDs (`request_context_reconciliation_tests.rs`,
`33f92d40391`) stay committed and ignored until their runtime clears API fit.

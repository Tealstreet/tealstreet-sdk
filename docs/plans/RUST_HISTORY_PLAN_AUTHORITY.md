# Plan and Slot history authority

Status: plan only; API fit review required before source. This is a separate
engine follow-up to original-root history bridge #1848, landed as
`6872554ac85244ec7b768b79af7187013dba429a`. Its ReadCompletion constructor and
original Read-root authority remain unchanged. Plan and Slot are opaque engine
owners, never original-root Context substitutes.

## Consumers and ownership

The first consumer is BloFin private general history and closed positions.
claude-4yjlw0 owns the signed-secondary dispatch migration and native recipe;
codex-9czdjh owns unsigned/PublicTrades and the native history contracts.
Their regions are history.rs and closed.rs, with lib.rs callback integration
after the PublicTrades consumer releases it. Do not edit those regions in this
engine PR. Coordinate background/mixed-leg dependencies with claude-hcmq3f.

BloFin native requirements: signed Secondary reads with passphrase fallback,
nonempty-page continuation, at most 50 pages per family and unchanged-cursor
refusal. A signing/preparation refusal is strict and typed, with no retry or
auth inference; closed preparation failure must not enter response recovery.
Actual later-page transport failure retains accepted-only recovery under the
native retry contract. Obtain the exact per-family timeout, retry and combined
query geometry from the native owner before constructing any Plan declaration;
a page cap alone does not establish a whole-query deadline or resource bound.

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

API choice for uufub4 review: a Plan-specific admission guard may hold a mutable
Cx borrow without exposing it, allowing checked settlement followed by an
infallible shared-core acceptance. Alternatively use the existing shared
prepared-settlement pattern with a private one-use token. Do not weaken the
original Read guard or add unchecked mutator twins. Pin atomicity and borrow
scope in a real callback control. Diagnostics happen after dropping the guard;
further staging requires fresh admission. Native page selection/next send and
timer continuation still use Plan slots and current shared authority.

### Terminal and failure contract

Terminal validation occurs before captured finalizers, incomplete markers and
Plan settlement or seal. Reuse the one shared prepare/commit Plan terminal
funnel; no fallible projection/storage work follows a published marker. A denied
owner produces no marker, rows, codec/finalizer effects or terminal and cannot
retire a different current slot. No numeric retry follows an authority denial.

Successful final DATA may retire only unissued remainder under existing Read
conditional-completion rules. A later actual released transport failure may
produce native accepted-prefix recovery only through the new reviewed Read
failure seam. It must prove exact selected Slot/Context/Attempt failure custody,
retained owner authority, and the history core's accepted-only evidence. Failed
page rows/identities are excluded before retry or settlement. First-page failure
and zero accepted filtered rows do not manufacture success. The first missing
coverage cause and native final sorting/filtering/limits remain intact.

Preparation/signing/policy refusal stays strict even after accepted rows: no
retained result or incomplete marker, and supplied typed error fields pass
through without response/auth reinterpretation. Ordinary native errors follow
the consumer's existing strict-versus-recoverable distinction. Root expiry and
physical attempt expiry are distinct: review current shared expiry semantics
before permitting a retained-result terminal. No new lease, deadline extension,
retry credit, send permission or failure-to-DATA conversion is introduced.

## Original REDs and required controls

Commit compiling semantic assertion REDs against the landed source before the
implementation. Exercise genuine command entry, Preflight, Plan admission and
real Prepare/Bind/Claim rather than fabricated Contexts or registry internals.
A missing proposed API compile error is not original RED evidence. A legacy
numeric Completion under a genuine Plan is a negative reproduction fixture
only; it cannot become a production fallback. Preserve original assertions.

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
- Real later-page transport failure recovers accepted rows only where native
  policy permits; first-page and zero-accepted-row failures remain failures.
  Failed delivery never authorizes new rows, coverage or Store publication.
- Early DATA terminal retires only unissued remainder. Settled/selected sibling
  work cannot be skipped; marker/finalizer/storage preparation refusal leaves
  Plan/rows untouched and emits no marker.
- Cursor, range and multi-stage completion retain accepted-stage baselines,
  dedupe and the first coverage gap. No timer can admit response data; only
  shared Plan authority may permit delayed next-slot sends.
- Feature off preserves original-root/legacy behavior and excludes the new API.
  Mutation/unsupported Execution ownership is explicitly refused until reviewed.

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

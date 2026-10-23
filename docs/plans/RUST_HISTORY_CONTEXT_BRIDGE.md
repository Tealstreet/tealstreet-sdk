# Context-bearing history completion

## Scope and source baseline

Plan first on `feat/rust-history-read-context-bridge`, from actual SafeIDs landing
`7641650e7b1fc54c55593880ac16299171f60a90`. History ownership belongs to this
lane. Request lifecycle/registry/journal ownership remains with codex-uufub4.
The old unpublished `9f69413fb5c`/`6d0c7a2df3d` bridge is historical evidence,
not a dependency to merge wholesale: its mixed-mutation RED is now covered by
the landed read-companion validator. Refresh the design against actual #1792.

Production scope is the shared `engine/src/history.rs` ownership/admission seam.
Tests use real Engine/Cx managed prepare/bind/claim/response callbacks, and the
existing history, cursor, range and coverage suites remain compatibility checks.
No venue-specific executor, retry policy, signer, host, scheduler, Store freshness
fence, numeric fallback, Plan authority or execution/observer group is introduced.

Consumers have concrete requirements from Lighter general/history stages,
BloFin general/closed history, and BingX history after its signer and bounded
query-plan prerequisites. Their page algorithms and native response verdicts
remain venue-owned. A Context proves liveness and provenance, never native body
validity, semantic snapshot freshness, complete coverage or signing admission.

## Prerequisite and root contract

Actual #1792 exposes `validate_read_http_delivery(root, leg)`,
`validate_read_completion_context(root)`, `done_context` and `fail_context`.
The first additionally checks the exact original completion root, while the
companion validator alone accepts other issued Read handles. Binding a child
leg as if it were an original root must fail even for a zero-I/O terminal.

Agreed narrow prerequisite, owned by uufub4 in the request registry:
`Cx.validate_read_root_context(&Context) -> Result<(), request_context::Error>`.
It composes existing read-companion issuer/owner/generation/no-mutation checks
with exact original-root equality; it changes no expiry or completion semantics
and grants no send rights. Consume only its reviewed committed pin, and obtain
any required exact-stack authorization before integrating an unmerged API. No
root identity or journal-policy copy belongs in history.

## Proposed API: one owner, distinct legacy and Read authority

Use sealed authority parameters with legacy defaults on the existing types:

```rust
Completion<A = LegacyAuthority>
HistoryRows<T, A = LegacyAuthority>
CursorHistory<T, A = LegacyAuthority>
RangeHistory<T, A = LegacyAuthority>

type ReadCompletion = Completion<ReadAuthority>;
type ReadHistoryRows<T> = HistoryRows<T, ReadAuthority>;
type ReadCursorHistory<T> = CursorHistory<T, ReadAuthority>;
type ReadRangeHistory<T> = RangeHistory<T, ReadAuthority>;

ReadCompletion::from_context(cx, command, original_root) -> Result<Self, Error>
ReadCompletion::root(&self) -> &Context
ReadCompletion::validate_delivery(&self, cx, exact_page_leg) -> Result<(), Error>
```

`LegacyAuthority` retains the current numeric completion behavior. Its public
constructor remains `Completion::new(command)`. `ReadAuthority` is opaque and
constructed only through the exact-root validator, matching the numeric command
for diagnostics. It retains the same immutable original Read Context and deadline
through every page and composite stage. Sealing prevents a venue from inventing
a new authority implementation or converting Read ownership into a legacy owner.

The common row/paging algorithm is shared, not copied into a second managed
executor. Existing legacy stage/accept/fresh/advance signatures remain available
only on the legacy authority instantiation. A Read owner exposes one borrowed
`admit(&mut self, cx: &Cx<P>, exact_page_leg: &Context) -> Result<Admission, Error>`
guard for its row/cursor/range core. The guard validates
`validate_read_http_delivery(original_root, leg)` once and holds the owner and
immutable Cx borrow for the synchronous callback. Its mutators delegate to the
same shared stage/fresh/admit/advance/coverage algorithms as legacy admission;
there are no checked twins of every mutator and no alternate executor.

An unguarded legacy admission method on a Read history is a compile error. The
guard cannot move with the owner into the next Pending or outlive the callback.
Record the page leg on the first page-mutating operation; a different leg cannot
stage/admit a previous failed prefix or its identity/coverage state. Binding records the logical leg and exact physical Attempt
per staged page, not per row. Clear it on validated admission, safe discard or
stage restart. Safe discard removes only unvalidated staging and its leg binding.
Native response validity is still a separate condition before accepting a page;
existing native retry/fallback identity behavior must remain explicit.

Admission exists only inside a managed HTTP delivery callback, including its
failure callback. Empty failed-alternative transitions (WOO/TxFlow patterns) use
that failure delivery, not a later timer. Timer-resumed continuations do not
advance a second time; terminal-from-timer uses only root completion validation.
A denied admission changes no output, dedupe, page facts or coverage. It grants
no native body truth, physical send or retry permission.

Read-only inspection and pure request selection share the current algorithms.
`step()` changes only selection bookkeeping; cursor/pages inspect it.
`with_request_budget` is construction-only, before any fetch, as currently
enforced. `restrict_active_range` is pre-send geometry selection, not response
admission: its only production caller is Phemex income after step()->Fetch,
clipping the selected padded interval to the caller's command end before
income_send. Preserve its finite/narrowing-only/active/empty-staging invariants.
Inputs must derive from command bounds or selected geometry, never native
response data; any future response-derived restriction requires admit(). It may
run while a previous callback selects its next request, which is still pre-send
for that next leg. Managed prepare/claim remains the sole send authority.

Pure inspection/selection, safe discard and stage restart do not replace the
root or confer publication/send rights. Venue-native cache/Store publication
outside the row owner must validate the same exact root/leg immediately before
synchronous publication, and independently satisfy native/freshness fences.

Independent API review accepts sealed typed authority and legacy-default
constructor compatibility, and recommends this single borrowed admission guard
instead of checked twins. Only this mechanism will be implemented. Consumers
receive a concrete reviewed API pin before adapting source.

## Finalization and error behavior

Legacy finalization returns `()`, as before. Read finalization returns
`Result<(), request_context::Error>` through the sealed authority's terminal
result type. A venue migrates its history owner as a whole; it does not mix
legacy and Read terminal result types through an ad-hoc generic local adapter.
A Read terminal Err is handled or deliberately ignored as ownership denial,
never unwrap'd and never retried through numeric done/fail.
The Read constructor uses the exact shared read-root check. Every terminal
path explicitly calls validate_read_completion_context before done_context OR
fail_context, including typed failure, strict rejection and encoder failure.
Neither terminal API itself supplies the no-mutation read-companion check.
A dedicated mixed-mutation failure-path control proves that a denied Read reject
cannot consume the sibling mutation owner's terminal authority.

Before a custom finalizer receives accepted rows, validate completion authority;
a stale owner must not run a captured native-cache side effect. Immediately
before `history.incomplete`, revalidate read-companion authority, then emit the
existing command-scoped Warn and call `done_context` synchronously with the
original root. No yield or request occurs between validation, marker and done.
Error paths publish no marker and use `fail_context`, preserving explicit native
kinds and the existing default Internal classification.

Preserve accepted-only partial recovery for actual later-page transport/native
failures, strict endpoint-policy rejection even after accepted rows, first-gap
retention, final sort/dedupe/limit/encoding, and one owner through restart_stage.
Managed preparation/policy/lifecycle refusals are strict rejects; they cannot
turn into successful retained history. BloFin's current closed-history signing
refusal recovers accepted rows, unlike its general history path: any change at
that consumer requires its own documented caller change and regression.

A current logical completion after its deadline remains whatever the registry
permits; it never revives data admission from an expired physical leg, retries,
or new send permission. Background, mutation, mixed-mutation, foreign issuer,
stale generation, wrong original root and wrong current delivery refuse before
any data, native effect, marker or terminal escapes. Duplicate owners become
silent/denied after the first consumes the generation.

## Regression and two-venue proof

Commit original assertion-RED controls against the actual existing numeric
Completion while retaining genuine managed Context/leg callbacks. Pin valid
managed completion lost through numeric finalization, a revoked marker leak,
wrong-root/child binding, mixed-mutation refusal and side-effect-free denied
admission. Do not call a missing API or compilation error an original RED.
Then migrate only the construction/admission seams to the reviewed API.

Core controls cover success, incomplete Warn before done with Error-only log
filter, no accepted rows on first failure, accepted-only later failure, strict
reject after accepted page, malformed page prefix exclusion, serializer error,
retry discard, cursor/range stage ownership, exact page-leg binding, foreign
issuer, background/mutation/mixed owner, reused numeric ID, stale generation,
sibling/current wrong delivery, expired physical admission versus permitted
logical completion, duplicate terminal, and zero captured finalizer/publication
side effects on denial. Retain all legacy history suites unchanged.

Two real venue proof receipts are required before readiness, coordinated with
Lighter (codex-zs5e83) and BloFin (codex-9czdjh), with BingX (5c57jl) as another
consumer once its checked whole-query Plan bound is available. Consumers may
validate against an owner-authorized exact reviewed bridge stack; do not label
synthetic engine adapters as two production venue migrations. If their distinct
Plan/signer prerequisites remain unavailable, publish the reviewed bridge draft
with that exact proof gap and keep it unready. No guessed timeout/page bound or
private signer compatibility path is permitted to manufacture the proof.

BingX history migration removes history callers of its temporary sign_http stage.
The shared stage also serves listen-key POST/PUT upkeep, so final physical
allowance removal additionally requires claude-55ao7t's background-session Context.
This bridge alone does not close that allowance, joined metadata completion,
Plan legs, observer fan-out, legacy history inventory or NX deployment work.

## Verification and review

Run focused engine history/managed callback suites, original RED and corrected
GREEN controls, strict touched Clippy/fmt/bans and exact request-inventory tests.
Use Cargo concurrency four. Independent plan/API/source/adversarial reviews must
check authority ownership, admission, denied-finalizer effects and legacy
compatibility. Actual venue integration uses the standing exact inventory prune
or same-site net-shrink rehash rules and its own caller section.

The same PR carries CALLERS text describing Context-only managed completion,
strict preparation refusal and command-scoped incomplete handling as applicable.
No merge/proof status belongs in caller prose. Fresh CI/artifact proof binds the
actual dependency pins; no historical acceptance is inherited. The owner grants
a landing window, followed by announcement, fresh separate exact-scope guard,
separate head-matched merge and actual-tree audit. No merge is authorized here.

Session coordination confirmation (claude-55ao7t): history and listen-key
callers have separate migrations; the physical legacy sign_http allowance is
deleted only after both land, by whichever migration lands second. Session
source stays outside this lane. Lighter additionally requires unchanged early
input/keyless compatibility, strict account-auth/native-budget refusals after
accepted rows, trade-to-funding stage baselines and zero-filtered-row failure.

Admission-guard borrow discipline: scope the guard only around admission, drop
it before `&mut Cx` diagnostics (for example ROW_SKIPPED), Store publication or
reject/finish, and re-admit if more staging follows. The immutable Cx borrow
prevents mutation declaration, revocation or completion between validation and
page mutation. Add an engine control for admission -> drop -> diagnostic ->
re-admission; document this consumer idiom in the same-PR API/CALLERS section.

Original actual-source control checkpoint: three tests compile and fail by
assertion against unchanged `7641650e7b1` history source. The fixture retains the
real original root and current child leg through managed Prepare/Bind/Claim and
response delivery, but its owner-construction seam still uses numeric
Completion::new. Valid delivery loses its proper terminal; duplicate numeric
owners cannot produce the expected single correct result; a sibling mutation
case sees an unauthorized history companion before the proper mutation owner
finishes. The existing four legacy history suites pass 23 controls. These REDs
are intentional regression evidence, not a ready branch or implementation claim.

Pre-audit found a real Read-only identity leak: delegating guarded fresh()
directly to CursorWalk::fresh committed identities from an unvalidated failed
prefix, so a retry dropped those rows as duplicates. Two assertion-RED controls
pin that leak; a third accepted-stage compatibility control passes. Read
authority now holds page-local identities, tests freshness against committed
walk identities without mutating them, commits the set only on validated
advance, and discards it with the failed staged page. CursorWalk gains only a
crate-private read-only contains_identity query. Legacy direct fresh/discard
behavior is unchanged. Full managed delivery/retry integration remains required
after the exact-root constructor dependency is available.

LegacyAuthority is opaque with a private zero-sized field rather than a public
unit constructor, preventing direct calls to the sealed terminal trait as a
disguised numeric completion. Compile-fail examples name E0599 for unchecked
Read mutation/numeric construction and E0451 for authority construction.

Core callback controls use a cfg(test)-private authority wrapper only around the
first genuine command Read root issued immediately at fixture entry. They do not
certify constructor binding or a production venue migration. Fifteen controls
exercise real Prepare/Bind/Claim and exact managed callbacks: two-page recovery,
strict reject/encoder failure, page binding and discarded-prefix identity reuse,
readmission after mutable diagnostics, mutation-sibling failure/finalizer denial,
transport failure, physical expiry versus logical timer completion, single-page
row admission, inclusive range probes and stage retention, filtered-row refusal,
pre-request typed/empty completion, duplicates and reused command generations.
The synthetic response rows are core test facts, not native codec proof. The
public constructor and two real venue receipts remain distinct dependency gates.

The same-leg physical retry control calls the shared http_retry_context with
ReadFailure evidence, preserves the exact logical Context/deadline, and queues a
new managed preparation/claim. It proves discarded identities are reusable even
when Context equality does not distinguish the two physical attempts. Replacing
only guarded fresh() with direct CursorWalk::fresh reproduces an assertion RED
in that control (the retry row is incorrectly not fresh); restoring the exact
source returns GREEN. This is an implementation-regression mutant, separate
from the original landed numeric-Completion RED evidence.

Constructor binding is a read-only authority check, not an exclusive claim or
new registry reservation. More than one ReadCompletion may bind the same valid
original root; only the first accepted context terminal wins. Every later
terminal revalidates and emits neither a marker nor a result. Pin this explicitly
using the public constructor once its shared validator is available. The real
failure-callback matrix also stages an unvalidated prefix and finishes without
discard; accepted-only recovery must still exclude that staged prefix.

Adversarial retry refinement: Context equality identifies the logical leg, not
the physical attempt. A fifteenth managed control deliberately omits discard,
retries the same Context, and requires admission to refuse before fresh/stage;
it currently reproduces an assertion RED against the local bridge checkpoint.
The correction binds each staged page to both Context and the current opaque
Attempt. A crate-private Cx accessor delegates to the existing shared read HTTP
delivery validator and returns only that validated delivery's Attempt. It grants
no additional authority and copies no registry policy. Admission captures this
token under its immutable Cx borrow; a different sibling or physical attempt
returns State until explicit safe discard clears the binding and pending ids.
The same-leg no-discard refusal and explicit discard/re-admission positive are
both required. This accessor is coordinated in uufub4's registry region before
source edits; the original-root constructor dependency remains separate.

Attempt-binding correction checkpoint d39711cac42 has independent adversarial
source and registry-owner static reviews CLEAN. The fifteen managed core tests
pass, as do 604 engine tests with all features, six history doctests and strict
all-target/all-feature engine Clippy. The original three numeric integration
regressions remain RED until the public constructor migration. The old reviewed
#1830 2d8de248f64 is integrated only as a local predecessor; constructor exposure
and publication await the owner's single announced composed #1830 pin, after
which integration and verification must bind that exact source. No final CI,
native two-venue receipt, public constructor readiness or merge is claimed by
this local checkpoint.

Canonical #1830 66cb32e89588e60bd736d3ff121f5ce52bfc4f3b is genuinely
integrated at a4aa803eb63. All incoming unowned source is exact; registry differs
only by the separately reviewed crate-private physical-attempt accessor. The
CALLERS conflict retains the incoming full-file prefix and this bridge's added
section verbatim once. The public from_context(cx, command, root) constructor
now delegates validate_read_root_context and checks the exact command id before
retaining the immutable root. It grants no send rights or exclusive reservation.

The three original numeric regression assertions pass after changing only owner
construction, guarded admission and contextual terminal handling. Eighteen core
controls separately cover the public constructor's wrong id, child, foreign,
background, mutation/mixed and post-terminal denials, duplicate binding, stale
reused generation, logical completion after physical expiry and live versus
expired timer-issued next legs. The public constructor tests use genuine engine
admit_http_command entry. The older private wrapper remains solely a core-test
fixture, not constructor proof. Two actual venue consumer receipts and agreed
Plan/observer authority remain distinct readiness gates; this single-Read-root
bridge cannot cast or consume opaque Plan slots.

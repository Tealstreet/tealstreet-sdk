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
only on the legacy authority instantiation. Read instantiations have checked
variants taking `cx` and the exact page leg, returning `Result<_, Error>`:

- HistoryRows: stage, accept_page and coverage admission.
- CursorHistory: fresh, stage, advance, advance_probe and advance_with_coverage.
- RangeHistory: row/fact staging and every accept_page/where/coverage admission.

Each checked entry validates `validate_read_http_delivery(original_root, leg)`
before changing staging, dedupe, accepted output, paging coverage or cursor
state. Staged rows remember their page leg; a sibling/retry leg cannot admit a
previous failed prefix. Safe discard removes only unvalidated staging and its
leg binding. Native data validation still precedes admission. Retry/fallback
identity semantics remain those documented by the existing cursor owner.

Pure read-only inspection, request selection, safe discard and stage restart
share the current algorithms. They do not confer publication/send rights or
replace the retained root. Venue-native cache/Store publication outside the row
owner must validate the same exact root/leg immediately before synchronous
publication, and independently satisfy its native/freshness fences.

An alternative to audit before source is the earlier runtime-authority enum plus
borrowed page guards. The preferred typed API makes use of an unguarded legacy
admission method on a Read history a compile error, with no legacy footprint for
managed-state flags. Do not implement both mechanisms. API review settles this
choice before implementation; consumers must receive a concrete reviewed pin.

## Finalization and error behavior

Legacy finalization returns `()`, as before. Read finalization returns
`Result<(), request_context::Error>` through the sealed authority's terminal
result type. Caller ownership denial never retries numeric done/fail. The Read
constructor and every terminal path use the shared read-root/read-companion
checks, including typed failure, strict rejection and encoder failure.

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
allowance removal additionally requires uufub4's background-session Context.
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

# Position type hardening

Addendum: `/Users/sam/briefs/types-hardening-addendum.md`.
Audit base: `de074404342f645eedfc60e7a6c6a2ee9470f7f7` (PR #3306).

The audit covers the 120 unique Position / DisplayedPosition compiler-census
files in [the plan](TYPE_OWNERSHIP_P1.md), plus the new Position frame reader,
OEMS bounding, CLI substitution, and the compiler
contract gate (124 paths total). Unrelated transport payload types in those
files do not become Position and are outside these counts.

The work has three steps: inspect assertions against the generated contract;
remove only those justified by actual types or narrowing; check compilation,
existing consumer tests, and the diff before publishing counts. Cross-slice
files remain integrator-owned. Their changes are delivered in
[SHARED-PATCH](TYPE_OWNERSHIP_P1_POSITION_SHARED.patch), not applied to those
source files in this commit.

## Counts

Counts compare syntax nodes with the audit base, not grep matches in imports,
comments, or generated strings. A cast containing an `any` annotation contributes
to both the cast and `any` columns. `unknown-as` means a double assertion through
`unknown`; import aliases and boolean negation do not count.

| Delivery | Casts removed | Non-null assertions removed | `any` removed | `unknown-as` removed | Suppressions removed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Applied source | 22 | 33 | 2 | 0 | 4 |
| SHARED-PATCH, awaiting integrator | 26 | 2 | 0 | 0 | 0 |

There are no new cast expressions, non-null assertions, `any` annotations, or
suppression comments in either set. The compiler contract gate now asserts the
four expected diagnostic codes and locations directly (nullable arithmetic, readonly field,
nominal enum, missing required field) instead of suppressing their errors.

The tests retain their existing Position values. Complete factories use their
return annotation, fixture accesses narrow required state, and the reactivity
test creates a mutable input before passing it as Position. Shared changes use
narrowed filter results, precise numeric-column keys, and readonly inputs for a
function that only reads positions. No venue defaults or unknown-price policy
is changed.

## Deferred: 18 sites

These sites remain after the ready shared patch is applied; pending shared
removals are counted separately above.

These are **16 incomplete Position fixture assertions**, one undeclared market
field assertion, and one pre-existing public `any` contract. Lines below refer
to the audit-base commit, so they remain stable when the shared patch is applied.
The incomplete fixtures deliberately omit required fields; choosing replacement
values can affect the scenario under test and is not a mechanical cast removal.

| File and line | Sites | Reason |
| --- | ---: | --- |
| `packages/cli-core/src/commands/trading/ChartCommand.test.ts:10` | 1 | Partial chart Position omits identity, notional, leverage and liquidation. |
| `packages/cli-core/src/commands/trading/MonitorCommand.format.test.ts:67,88,117` | 3 | Formatting fixtures omit required Position fields; decide a complete fixture contract first. |
| `packages/cli-core/src/commands/trading/PositionCommand.test.ts:17` | 1 | Helper accepts `Partial<Position>` and coerces it for display-command scenarios. |
| `packages/cli-core/src/commands/trading/SwarmCommand.test.ts:427` | 1 | Size-only Position behind an `unknown` assertion; adding price/identity data changes the fixture premise. |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.live-smoke-leverage.test.ts:12` | 1 | Leverage-only snapshot deliberately lacks numeric Position fields. |
| `packages/safe-cex/src/__tests__/audit-regressions/bitunix.remaining-audit.test.ts:418,435,452` | 3 | Close-position tests use identity/size-only rows; complete numeric fixtures need scenario review. |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.private-recovery.test.ts:139,141` | 2 | Snapshot ordering uses id/PnL-only rows; replacement fixture values need review. |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.leverage-side.test.ts:78` | 1 | Side-selection fixture is an incomplete two-row Position array. |
| `packages/script-api-core/src/adapter/createCoreAdapter.test.ts:49` | 1 | Adapter fixture combines a bare-string side and omitted required fields; revise its fixture API separately. |
| `packages/script-api-core/src/order/prepareOrder.reduceLock.test.ts:29` | 1 | Reduce-lock fixture supplies only symbol, side, size and mode. |
| `packages/safe-cex/src/webworker/postProcessors/DisplayedMarginPostprocessor.test.ts:9` | 1 | Margin aggregation fixture deliberately supplies only side/notional. |
| `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.ts:320` | 1 | Reads undeclared `Market.maxLeverage`; switching to limits or dropping the field is a behavior decision. |
| `packages/safe-cex/src/generated/Position.ts:25` | 1 | `info?: any` preserves the existing public arbitrary-info contract. Changing it to unknown needs a public API decision and Rust generator change, not a handwritten generated-file edit. |

Other casts that construct incomplete ExchangeLike/CommandContext test doubles,
and assertions about raw venue transport envelopes, are not made unnecessary by
the generated Position contract and are not included as Position removals.

## Verification

- AST comparison checks every changed source file for introduced type escapes.
- The Position compiler gate checks four expected failures without directives.
- Additional compilation covers changed tests normally excluded from package
  typechecks. Existing unrelated CLI test diagnostics are recorded separately;
  the hardening must introduce none.
- Shared changes are tested together before producing the patch. Applied source
  is then checked on its own with root typecheck, lint, and tests.

Detailed command results are recorded in PR #3306 and the worker report. The
shared patch has not been merged by this worker; NX and CLEAR hostile review
remain prerequisites for merging the PR.

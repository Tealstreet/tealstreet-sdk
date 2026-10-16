# Extractor scope audit — 2026-10-12

This is a read of `src/extract.ts`'s filters, not a claim that unlisted behavior
has no porting obligation. The exported mutable-binding fix adds exactly three
rows: HOW's howWebSocketSG/howWebSocketJA and safe-cex's currentProxyToken.
Measured before reconciliation: 6,901 → 6,904 rows across 474 files, no orphans.
All three additions are pending, empty rs/layer/note. Existing annotations stay
unchanged. `variable` and `variable-fn` distinguish mutable bindings from consts;
let and var share these kinds. Their lifetime/assignment behavior is not inferred
or validated by this inventory.

## Files the inventory does not visit

- Only the three explicit sourceRoots: safe-cex/src, http-over-websocket/src,
  script-api-core/src/order. Program dependencies outside these roots may affect
  type checking but receive no rows here. A missing root directory is skipped.
- Only `.ts`: `.tsx`, `.mts`, `.cts` are outside the filter. `.d.ts`, `.test.ts`,
  any path segment `__tests__` or `__snapshots__`, and basename `perfFixture.ts`
  are excluded. `.spec.ts` and other test-like names are **not** excluded by name.
- Files are walked independently of tsconfig's file exclusions and added to the
  Program. Symlinks throw; syntactic/semantic diagnostics throw, not silent skips.

## Declarations without their own rows

- Imported reexports get no local duplicate rows; local aliases are indexed by
  original declaration name, not every export alias. Only same-file declarations
  from the compiler's export set seed export ownership.
- The top-level dispatch handles variable statements, function/class declarations,
  exported type aliases/interfaces and exported enums. It does not recurse into
  namespaces/modules or arbitrary top-level blocks. Export-assignment expressions
  (for example `export default () => 1` or a default object literal) have no
  standalone handler. Default-exported identifiers can still resolve through the
  compiler alias to an inventoried local declaration; `export default start` in
  store.webworker.ts is already `start`, kind `const-fn`. Named/anonymous default
  function/class *declarations* also have handlers.
- Private data bindings, private type aliases/interfaces/enums, and unreachable
  private functions/classes are absent. Const callables and function/class
  declarations can enter through same-file reachability; non-exported mutable
  bindings remain excluded in this narrowly authorized change.
- Reachability starts from exports and top-level expression statements, follows
  same-module identifier symbols, and does not create rows for anonymous inline
  callbacks, nested function declarations or arbitrary initialization statements.
- Non-callable class fields and constructor parameter properties, static blocks,
  interface/type members, enum members and ordinary object data properties have
  no separate rows. Their enclosing class/type/enum/value may have a row.
- Class callable fields typed outside the package are excluded unless an
  initializer/assignment supplies recognized local behavior. Recognition uses
  call signatures, syntactic functions, local function aliases, bind calls,
  callback arguments and method references; it is not dynamic execution analysis.
  Callability erased to any with no recoverable initializer can be missed.

## Provider member limits and explicit failures

- Member extraction visits direct properties of exported variable initializers
  that unwrap to object literals. It does not inventory members of arbitrary
  factory-returned providers, later property assignments or prototype mutations.
  Object accessors are not dispatched as class accessors; a non-callable getter
  has no separate row. Exported value binding coverage is not member coverage.
- Recognized class-expression providers, callable Object.freeze providers,
  callable spreads and nested callable objects throw an unsupported-shape error.
  These are fail-closed cases, **not** silently supported shapes. The guard is
  syntactic/type-assisted and is not a universal detector of every factory shape.
- Overloads sharing a compiler symbol collapse to one row. Conflicting distinct
  declarations with the same kind/name throw rather than sharing annotations.

This patch does not silently expand any of these remaining boundaries. In
particular, default export expressions, namespace contents, mutable private
behavior and dynamically built provider members remain scope gaps for a future
measured decision. A green coverage gate means this declared inventory reconciles;
it does not mean every executable TypeScript expression has an independent row.

## AST census of the suspected declaration forms

Measured 2026-10-12 at integration commit `d3e5f6537e`, across the 474 files
selected by the actual `sourceRoots`, `walkFiles` and `isSource` exports. Each file
was parsed with `ts.createSourceFile(..., ScriptTarget.Latest, true)` and every
node recursively visited with `ts.forEachChild`; this was **not line-anchored
grep** and did not depend on indentation or line breaks.

| Form | AST discriminator | Count |
| --- | --- | ---: |
| Default inline export expression | ExportAssignment, not isExportEquals, non-Identifier expression after unwrapping parentheses | 0 |
| Default identifier export | Same, with Identifier expression | 1 |
| Namespace declaration | ModuleDeclaration with NodeFlags.Namespace, excluding global augmentation | 0 |
| Identifier module declaration | Other identifier-named ModuleDeclaration | 0 |
| `export =` | ExportAssignment.isExportEquals | 0 |
| `declare global` | ModuleDeclaration with NodeFlags.GlobalAugmentation | 0 |
| String-named ambient module declaration | ModuleDeclaration with StringLiteral name | 0 |

The one default identifier export is
`packages/safe-cex/src/webworker/store.webworker.ts:632`, `export default start`.
A live `extractSources` run confirms `{ ts: 'start', kind: 'const-fn' }`; its
existing manifest row is pending. It is **not a missing symbol**. The AST counts
agree with the reported zero counts for the other listed forms.

These counts bound only those syntax forms on this source revision. They do not
prove that all other silent classes above, such as dynamically built provider
members or erased callability, have zero live cases. Roots and source revisions
can change; the census is not a permanent scope guarantee.

## Open field-level conformance gap

The known live field-class gap remains **OPEN**. The TypeScript extractor does
not assign separate rows to non-callable fields/type members, and the Rust
invention index records public items rather than a struct's public fields.
Consequently, a mapped/ported type can contain extra or missing fields without
an item-level correspondence failure. Gateway envelope review exposed this class;
a green item inventory does not substantiate field equivalence.

Closing it would require comparing the Rust struct's public fields against the
TypeScript type's members and accounting for the declared representation mapping.
That is a different check from today's item-level name/source correspondence.
No field-check design or implementation is commissioned by this audit; this
section records the known hole rather than implying it is already enforced.

## Validation of this change

35 extractor tests and 66 Rust parity tests pass. Controls cover let and var,
callable/data distinctions, destructuring, local export aliases, provider members,
missing-row check-mode refusal without writes, annotation-preserving reconciliation,
and value/callable correspondence refusal for wrong names. Restoring the original
const-only filter fails all three new extraction/CLI controls; restored tests pass.
Extractor check reports zero changes and the Rust check/report validate 6,904 rows.

Typecheck, ESLint and Rust Clippy pass. This worktree has no local TypeScript
binary at the path hard-coded by tsc-ts6.mjs; Node resolves the repository's actual
installed dependency from the parent checkout. Typecheck was run with that same
compiler directly, against this worktree's tsconfig and sources. CI installs root
dependencies before the extractor job; no dependency or install step was added.

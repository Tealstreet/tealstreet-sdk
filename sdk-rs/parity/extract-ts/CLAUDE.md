# TypeScript parity extractor

Use the repo's TypeScript 6 compiler API through a real safe-cex `ts.Program`.
See [the parity README](../README.md) for the contract and commands.

**Reconcile, never regenerate.** Preserve agent fields keyed by `ts` + `kind`;
keep orphan rows. Validate the complete existing corpus before any writes.
`--check` must not write files or create directories. Tests cover metadata
preservation, no-write failures, and Bybit/BaseExchange callable instance fields.
Coverage fails independently for added symbols, orphaned symbols, and changed
files; never infer symbol additions from serialized text changes.
`rs` accepts a string or an ordered array of strings. Preserve its shape and
member order through parsing, serialization, and reconciliation; malformed
arrays must name the symbol and abort before any write.
Optional `[file].rust_files` is ordered agent-owned provenance metadata. Validate
exact relative Rust file paths, reject duplicates, and preserve absence, empty
arrays, member order and byte-stable canonical formatting through reconciliation.

Extraction checks Program semantics before reconciliation: unresolved callable
types must fail loudly rather than silently shrink the symbol count. Class and top-level function
reachability follows same-module compiler symbols from exported declarations.

The package is a root Yarn workspace with lint, typecheck, and test-ci gates;
dependencies use the root lockfile. The root parity scripts and Rust parity
sources have separate ownership.

**Phase roots** live together in `sourceRoots` in `src/extract.ts`. Preserve
legacy safe-cex manifest paths; added packages use reserved manifest prefixes
(`script-api-core/order/`, `http-over-websocket/` for P2b). Both mapping directions reject ambiguous paths.
Adding a root must preserve annotations, orphan reporting, deterministic output,
and check-mode write isolation across every root.

**Exported values are declarations.** A top-level exported non-callable `const`
binding has `kind = "const"`; callable consts retain `const-fn`. This includes
primitive values, arrays, objects, singleton instances, destructured bindings
and locally declared aliases exported by name. Imported reexports, private
data constants and data properties remain excluded. Provider function rows
remain separate from their owning value declaration. Inline Rust values need
an explicit adapted mapping/note rather than silently vanishing from coverage.

P3b groups three narrow candleCache roots for web/server/react-native with
disjoint manifest prefixes. Keep the legacy safe-cex root first: reverse-path
fallback and sourcePrefix depend on it. New runtime backend rows reconcile
through the same contract, rather than being hand-written outside the roots.

Exported `let` and `var` bindings are included as `variable` (data) or
`variable-fn` (callable). They use the same original-declaration identity, alias,
destructuring and direct-provider rules as exported consts. Non-exported mutable
bindings remain excluded. These kinds do not claim Rust lifetime equivalence.
See [SCOPE_AUDIT.md](SCOPE_AUDIT.md) for the remaining structural boundaries;
reconciled coverage must not be described as an inventory of every expression.

P5 primitive extraction introduced poseidon-crypto/src and lighter-sdk/src/signer. Lighter venue composition now expands the latter to all lighter-sdk/src, retaining the existing lighter-sdk/signer manifest paths and adding client, constants, types, utils and the default API declaration. Count and report added rows whenever this closure changes. Poseidon TS rows cover loaders around src/wasm, not the Go cryptography; only the committed Go-WASM harness is evidence of that arithmetic.

The shared packages/json-parse/src functions use the disjoint json-parse/ manifest
prefix. Their complete parse/null contract, including the large-integer regex
corruption, is shared by HOW and venue transforms rather than exempted.

The Rust CI self-audit resolves the extractor dependency in isolated fixtures;
it does not require a hoisted checkout installation. CI also invokes
`scripts/check-clean-install.mjs` from the repository root before the unchanged
full root install and maintained-source semantic coverage. This supplemental
proof installs the three maintained runtime selectors with the copied lockfile,
checks decoded one-source fixture rows and preserved annotations, and requires
named new-symbol and missing-provider failures followed by restored checks.
It does not establish full workspace installation or maintained-source closure.

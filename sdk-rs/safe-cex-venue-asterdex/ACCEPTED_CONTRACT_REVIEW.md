# Accepted contract review

The first batch selects eighteen concrete function owners from the original
native/default census at source base `7d65a68076`: eight auth/type helpers, nine
API request/error helpers and the virtual-clock server-time parser. It leaves
166 census owners and all eight aliases for further review. This selection does
not establish local-owner coverage, a feature/WASM matrix or a whole-package
accepted-set gate.

Each contract pins the exact fresh compiler API and invokes that production
function. The 87 expectations were authored from the maintained TypeScript
bodies before executing Rust. Fixtures adapt the actual typed signatures;
they add no production JSON entrypoint. Native projections inspect optional
fields, enum variants, returned keys, strings, booleans and binary64 bits.

The reviewed domains retain the canonical
[UTF-8 string](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[typed-output](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[owned-snapshot](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and
[numeric-ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect) boundaries.
Existing differences keep their owning manifest rows under the maintained
[divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

The auth contracts distinguish canonical STP strings, the complete hex/prefix
rules, auth-field precedence, whitespace-only fallbacks, explicit API versions
and inferred credentials. String inputs retain the canonical UTF-8 boundary.
The API contracts observe own-key order, array entries, omission of the six
signing fields, null/empty filtering, query text without URL encoding, UTF-16
signing-key order, the source prototype assignment behavior, method/path pairing,
suppression short circuits and builder-error rewrites. The clock parser retains
the Number/string gate, positive finite constraint and native number bits.

`scripts/accepted-helper-source.cjs` executes the actual TypeScript bodies and
checks all eighteen owners against those 87 fixed expectations. It reuses the
maintained source/API loaders and extracts the unmodified `parseServerTime`
arrow. It does not execute exchange construction or claim live venue access.
The observed source run passed under Node 26.7.0 on arm64.

`scripts/accepted-helper-contracts.mjs` first checks those source expectations,
then runs fresh native/default compiler discovery and production `runProbes`
with one Cargo job. It requires exactly eighteen runtime receipts and refuses
classification-only receipts. It uses `loadContracts()` and the per-crate policy
shards through `loadPolicy()`; no flat policy maps are restored or modified.

`scripts/accepted-helper-controls.py` compiles two isolated production mutations:
case-insensitive STP admission and admission of zero server time. Each must reach
its named native owner assertion. Exact-byte restoration must pass every owner;
compilation errors are rejected as evidence. Run accepted probes serially.

The measured baseline passed eighteen production-native owners, with zero
failures and zero classification-only receipts. Both compiled controls produced
exactly one failed owner at the expected behavioral assertion. Both exact-byte
restorations passed all eighteen owners again. The maintained source expectations
also passed before every native run. Logs and generated compiler/probe artifacts
remain outside committed source.

## Flattened numeric ingress

The existing `live_types` tests `numeric_ingress_native_bits` and
`opaque_extensions_retain_native_numeric_values` both passed with ignored live
tests included, under the locked Rust 1.99 toolchain. The former directly calls
`serde_json::from_str` and `from_slice`, as well as owned/borrowed Value and shared
numeric paths, and inspects the actual fields' binary64 bits for decimal, negative
zero and overflow literals. The latter covers the same entrypoints for unknown
numeric extensions on all six flattened public records: market filters, markets,
positions, orders, tickers and websocket messages.

Those public decoders already capture numeric ingress before invoking their
inner schemas. This audit measured no public-path failure and makes no numeric
production repair. The six lexical inner `Wire` decoders are separate local-owner
obligations. These public-path tests do not classify or exempt those local owners;
the current batch remains limited to the eighteen selected helper functions.

After merging master `ff420e9212`, the frozen dependency install and the selected
source/native gate passed again: eighteen runtime owners, 87 source expectations,
zero failed owners and zero classification-only receipts. `parity:check` also
passed with 7,459 rows and 17,204 public Rust items. Both JavaScript entrypoints
passed syntax checks, and the working-tree whitespace check passed.

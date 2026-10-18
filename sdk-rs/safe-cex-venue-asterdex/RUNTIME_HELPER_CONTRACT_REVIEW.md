# Generic runtime helper contracts

This batch covers six additional Aster function owners. Each row pins the actual
compiler signature and executes the unchanged generic function with native shared
`Js` values. These are focused function contracts; the injected host supplies
property access, calls, spread and coercion primitives, not replacement helper
algorithms or a whole-exchange implementation.

| Owner | Native/source witness |
| --- | --- |
| `api_runtime::option` | Own, absent, undefined, null, inherited, signed-zero and object-valued fields; ordinary nullish-receiver TypeError and original getter failure. |
| `runtime_host::opt` | The same property cases, with optional access bypassing only null/undefined receivers; false remains a valid primitive receiver. |
| `api_runtime::clock` | Actual compiled dispatch retains clock receiver and sync arguments, original object results, negative-zero bits, call count and thrown identity. |
| `api_runtime::detail` | Message then optional-code evaluation and actual `api_helpers::get_aster_dex_error_detail` dependency; builder refusal text, fallback text and both getter failures. |
| `exchange_runtime::due` | Strict zero short circuit, repeated property access, exact interval edge, signed zero, string/null coercion, NaN and failures on either getter access. |
| `exchange_runtime::update` | Store method receiver, original rows, copied loaded flags, own undefined/false preservation, inherited-flag exclusion, unchanged old flags and ordered failure propagation. |

The 45 expected case results are fixed in the contract rows. The independent
`scripts/accepted-runtime-helper-source.cjs` locates and executes the maintained
TypeScript expressions: `options.apiKey`, `exchange?.options`, three virtual-clock
calls, the error-detail call/function, and the tick predicate and Store update.
The source runner observes getters, native identities and Number bits before
building its finite result. It neither reads Rust output nor substitutes a
rewritten TS helper algorithm. Generic Rust adapters specialize the actual source
keys (`apiKey`, `options`, `msg`, reconciliation time and `orders`).

The host's detail import uses the same real helper and finite dependency boundary
as the production binding. The clock callback and Store method are dependency
ports whose receiver/arguments/effects are asserted; this batch does not establish
network clock synchronization, full startup, or actual Store reconciliation.
Native property access, descriptor reads, spread and numeric comparisons use the
shared JS runtime. Observation serialization is confined to the finite test
result after retaining identities, getter errors and binary64 values.

`scripts/accepted-runtime-helper-contracts.mjs` discovers exactly these six owners
with pinned-toolchain rustdoc in `target/accepted-inventory`. The normal metadata
and canonical probe build retain their separate target paths. Canonical
`loadContracts`, `inventory` and `runProbes` require actual runtime receipts,
not classification-only permissions; no exemptions are added.

`scripts/accepted-runtime-helper-controls.py` temporarily changes six production
bodies: swallowed option errors, wrong clock method, wrong error field, an
interval edge shifted by one, a lost Store update call, and lost optional-nullish
short circuit. Each must compile and fail exactly its named native owner at a
function-output assertion. Compiler failures do not count. Exact source bytes
are restored after every fault, and every restore must pass all six owners.

These contracts obey the shared [string](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[numeric](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[output](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary) and
[alias](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) boundaries.
They do not cover other owners, local decoders or the full feature/WASM matrix.

The measured baseline on master `9c52bf430a` compiled and failed exactly the six
missing classifications. After adding the six rows, all six native owners and
45 maintained-source cases passed with zero classification-only receipts. On the
normal merge of `53befec583`, all six compiled faults failed exactly their named
owner with a function-output mismatch, and each exact restoration passed all
six owners and the source expectations again. The later `d72aa34492` merge changes
only BloFin; Aster, the shared JS runtime and these contracts remain identical.
The all-features cancellation/override/venue inventories then passed 7/8/25 tests
on `9ebd137174`. No production Rust source bytes differ in this PR.
Strict all-target Clippy passed for native and `wasm32-unknown-unknown` on the
same refreshed head. The final evidence note is documentation only.

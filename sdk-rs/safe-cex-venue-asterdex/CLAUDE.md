# AsterDEX Rust venue

Behavioral reference: `packages/safe-cex/src/exchanges/asterdex`; both runtimes
remain maintained. Check the [official API docs](https://github.com/asterdex/api-docs)
while implementing. The maintained TypeScript determines parity behavior.

Manifest contract: [MANIFEST-SPEC](../parity/MANIFEST-SPEC.md).
Cite the canonical [string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[closed output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[alias boundary](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[numeric requirement](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), and
[divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).
Keep implementation-specific costs and evidence in the owning rows and gate records.

`types.rs` uses private map-only wire decoders with whole-entry numeric capture.
`tests/live_types.rs` checks accepted/refused shapes and native numeric bits;
`NUMERIC_INGRESS_GATE_EXITS.md` records its bounded-depth acceptance cost.
Tables and auth/version helpers retain their separate live-source oracles.

`exchange_helpers.rs` owns pure mappers and decisions. Numeric metadata uses the
shared retained owner; `NATIVE_METADATA_GATE_EXITS.md` records its measured
preservation and refusal behavior. Prototype lookup witnesses execute the actual
source before serialization; shared TableHit carries lookup identity.

`account_fetch.rs` and `order_execution.rs` own account/order REST workflows;
`builder_approval.rs`, `virtual_clock.rs` and `listen_key.rs` own their respective
state and lifecycle. Public/private streams apply Store changes and callbacks;
`STREAM_FINDINGS_GATE_EXITS.md` records their effect-order and null/prototype cases.

`api_runtime.rs` owns Axios-compatible factories and their actual interceptor
bodies, including V1/V3/code signing and timestamp recovery. `exchange_runtime.rs`
owns construction, startup, safety reconciliation, disposal and static values.
`RuntimeHost` binds primitives and existing mapped dependencies; `AssemblyHost`
binds shared owners and existing member bodies. Neither permits a default
TypeScript evaluator or replacement whole-venue method implementation. Concrete
HTTP/backend composition is separate from these body tests.

The live oracle loads TypeScript through `SAFE_CEX_TYPESCRIPT_PATH`; its default
is the isolated target dependency package. Provision dependencies through
`safe-cex-types/ci-deps.cjs`, using an absolute target directory, and the project
Yarn workflow. The root `venue-oracle:asterdex` command runs the ignored live
oracle targets in CI. Local gates stay scoped to changed test targets.

`runtime-oracle.cjs` extracts maintained source bodies. Its library/base/socket
fixtures capture dependency arguments; they do not prove networking or crypto
primitive accuracy. Native runtime tests assert values, original handles and
ordered effects before wire observations. Source mutation controls and compiled
native mutations must fail named assertions before a passing gate is evidence.

`scripts/accepted-helper-contracts.mjs` runs the selected eighteen-function
accepted-contract batch against actual TS and production Rust. Its compiled
controls and remaining scope are recorded in ACCEPTED_CONTRACT_REVIEW.md.

`native_exchange::AsterDEXExchange` implements Exchange on the shared js_runtime.
Its private `binding` module supplies the existing RuntimeHost/AssemblyHost,
AccountFetch/OrderExecution and public/private stream bodies; it never evaluates
TypeScript. The HTTP factory receives each client's own transport options.
`tests/exchange_trait.rs` covers all 16 methods and V1/V3 signing; the inherited
scheduleCancel rejection and declared uppercase public-trade side boundary remain.
The registry supplies production native HTTP and gateway dependencies.
See BINDING_GATES.md for compiled mutation controls and scoped verification.


Native hosted signer handles stay in BaseExchange.signers and the JS options
receiver, outside structural options. Preserve the source's API-key requirement
and use the existing exchange_options decoder when inspecting fields beside a
runtime handle. Bare clock axios sends skip the shared remote adapter; account
clients keep the same hosted identity through post-sign transport selection.

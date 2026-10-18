# AsterDEX Rust venue

Cross-layer native venue/worker witnesses live in the parity tooling target
`aster_worker_emitter`. Private facade/frame checks remain in this crate;
AsterDEX has no dependency on the higher worker layer.

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
`FLATTEN_NUMERIC_REVIEW.md` describes direct raw-number witnesses for the six
public flattened records and copied lexical diagnostics for their six private
decoders. Those diagnostics do not classify or exempt local owners.

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

`STREAM_CONTRACT_REVIEW.md` describes the nineteen stream-function contracts,
their actual-source/native observations and grouped production-body controls.

`BUILDER_CONTRACT_REVIEW.md` records twelve additional builder contracts and
their source/native witnesses. The selected runner and six compiled controls
retain the separate obligations of lexical ordinary `Wire` decoders.

`RECORD_CONTRACT_REVIEW.md` records twenty ordinary wire record contracts,
184 native fields, 707 shape/field observations and a complete-array control.

`ENUM_CONTRACT_REVIEW.md` records eleven string/position-mode enum contracts,
their 173 raw-text/native observations and grouped String-frontdoor control.

`DECISION_CONTRACT_REVIEW.md` records eight account/exchange decision contracts,
their 98 fixed TS/native observations and five isolated compiled controls.

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

`ALIAS_CONTRACT_REVIEW.md` records the eight original native/default aliases.
The selected gate retains full discovery and authenticates actual compiler
artifacts before native Serde/conversion probes and compiled identity controls.

`PROJECTION_CONTRACT_REVIEW.md` records twenty nonce/builder/request/projection
function contracts, actual-source/native observations and isolated compiled controls.

Order cancellation forwards `__suppressAsterUnknownOrderError` through the shared
request config-extension carrier. The real API interceptors suppress only the
already-gone `-2011` response; cancellation still logs completion.
`tests/exchange_trait.rs` covers both HTTP-success and HTTP-error envelopes through
the constructed venue, signer, transport and event emitter.

Public transport errors call the stream's existing `on_close(None, "")` path,
matching the source error listener's `this.onClose()` call. This runs the existing
subscription reset wrapper and native reconnect without an invented close code.
`public_socket_error` proves master has no redial before the watchdog timeout and
requires one after the hook is installed. Shared scripted response data lives in
`tests/support/exchange_data.rs`; frame queues stay injectable at the transport
boundary while the production constructor, API and socket handlers run unchanged.

`fetchPositions` consumes raw `positionRisk` rows, matching the source's field
access and parseFloat coercions instead of requiring the declared 17-field DTO.
Sparse unused fields, boolean metadata and numeric leverage remain valid in both
V1 and V3. Missing notional remains NaN; raw info retains the original keys and
values through the shared numeric and owned-snapshot boundaries.
`tests/exchange_trait.rs::sparse_position_risk_rows_use_source_coercion_in_v1_and_v3`
exercises the constructed binding, signed clients and placeholder leverage.

The private socket inherits the shared primary-private reopen resync policy,
matching its TypeScript class. Do not opt out merely because the native host has
`enable_reconnecting = false`: the shared open listener still refreshes private
state on native reconnects. `exchange_trait::private_reopen_refreshes_orders_missed_during_disconnect`
closes/redials the real production socket and requires a new signed order read and
replacement Store snapshot. It fails without the inherited policy. The same target
covers private order/fill/position/balance frames through production handlers.

The live market-load binding ignores non-string leverage Map keys because they
cannot match a market's string symbol. Nullish/falsy initialLeverage values follow
the source transformMarket default of 10. The helper still retains original Map
identity, duplicate last-write precedence, log cardinality and partial entries
before a later parse error. `exchange_trait` compares fifteen payloads in V1/V3
against actual TS fetch/transform methods through the real production HTTP binding.
Leverage Map values retain their raw shared Js value through the source truthy
fallback. The approved numeric-limit policy then applies Number at the final max
field, preserving zero, signed zero, NaN and infinities without widening types.


Mark-price frames apply the approved strict Ticker row policy at the effective
Store target. A truthy non-number fundingTime refuses its complete row, logs its
symbol/field/original value and allows later siblings to update and emit. Other
mark fields retain source parseFloat coercion; falsey times retain omission.
Real Exchange socket regressions cover combined and bare arrays, complete bad-row
preservation, valid sibling events and unknown/inherited target no-ops.

REST candles map raw shared Js rows by the six fields actually read by TS.
The declared twelve-column tuple does not gate runtime response admission.
The real V1/V3 binding regression checks short rows, numeric prices, opaque unused
columns and missing-volume NaN against the maintained fetchOHLCV method.


The constructed ticker census in `tests/exchange_trait.rs::production_ticker_coercion_matches_constructed_typescript`
uses the maintained TS exchange and DefaultStore via `parity/scripts/ticker-strict-source.cjs`.
252 cases / 372 frames cover bare and wrapped ticker/mark batches, trade last and every validated depth price/amount input. The copied fundingTime field has twelve complete-row refusals; falsey times retain source omission.
The corpus includes null, strings, NaN-producing inputs, objects, booleans, missing
fields, raw numeric overflow and signed zero. It compares all eleven native number
fields before JSON; approved decision-3 refusal preserves the entire bad row.
`parity/scripts/ticker-native-controls.py` separately disables mapping and refusal
behavior, requiring a compiled native failure and exact restored pass.

`WIRE_CONTRACT_REVIEW.md` records the 42 private module-scope Deserialize
companions and their direct shape/native-value witnesses. The focused
`scripts/accepted-wire-contracts.mjs` retains all 608 observations and every
compiler field, including each separately executed public-wrapper boundary.

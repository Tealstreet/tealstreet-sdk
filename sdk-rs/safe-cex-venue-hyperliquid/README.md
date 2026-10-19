# Hyperliquid

The maintained TypeScript under `packages/safe-cex/src/exchanges/hyperliquid`
remains the behavioral reference. The frozen e6a8ffaf13 census is425ported,
106divergent and0pending across531rows, including spot. The current audit is
`sdk-rs/parity/HYPERLIQUID_DIVERGENCE_RESOLUTION.md`; older evidence/claim audits
record their original trees and do not describe the current backlog.

The standalone DTO cleanup removes declarations with no production Rust
consumer from `types.rs`, `spot_models.rs` and `spot_types.rs`. The current
554-row census is 390 ported, 87 divergent and 77 not-applicable. The latter
rows retain their TypeScript identity with empty Rust mappings. Production
exchange and socket bindings consume realm values; retained event models and
order aliases keep their existing native contracts.

`not-modeled-types.cjs` excludes the removed core roots from model and corpus
regeneration. Spot model generation retains four user-event roots. Their current
corpora cover 32 strict source cases; core corpora cover five literal roots,
43 records and 29 numeric roots (680 number cases). Native bit, field and
accepted-set assertions remain fixed for the retained roots. Earlier audit
reports and their original counts are historical evidence, not this census.

This crate contains typed payloads, action composition, L1/user EIP-712 signing,
order interpretation, pacing, API processing and host-bound exchange/socket
algorithms. Raw class construction and arrow dispatch retain opaque native
receivers over explicit host operations. A row census does not establish a live
browser/venue binding. Production `HyperliquidExchange::new` and the Spot constructor
below assemble the existing bodies through native hosts; their evidence uses
source executions, scripted transports and loopback HTTP/WebSockets.

`create_exchange_api` and `create_info_api` accept an adapter, error emitter and
shared clock-backed `Rc<RateLimiter>`. The caller must supply regional/proxy and
qs-repeat behavior; these factories neither install nor validate it. The shared
`HttpClientOptions.params_serializer` seam exists. The native registry supplies
these host inputs; `tests/native_host_hyperliquid.rs` checks the composed route.
See `CLAUDE.md#api-execution-contract` and `#limiter-execution-contract`.
Telemetry receives current request data without an extra parse before the provider.

Read retries use the shared Hyperliquid all-POST predicate. Relative exchange
writes do not retry; source absolute-URL/fragment misclassification is retained.
Signing/response routing uses exact endpoint equality, while weight classification
uses endpoint suffixes. Raw weight bodies are parsed once and only consumed fields
are decoded; ignored surrogate/deep extensions no longer change userRole weight.
The Value-returning parse helper retains its separately priced whole-return cost.

Raw typed capture preserves number lexemes and last-duplicate selection before
semantic value decoding. Native field assertions precede wire checks in the new
supplements. The selected public-field observations cite
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`,
`#forced-surrogate-conflict` and `#closed-typed-output-boundary`; the independent
capture limit is priced at `CLAUDE.md#record-capture-budget`.
A split300-unit response cap now reports representation refusal before emission,
without replacing the program-observed surrogate half. The source is unchanged.

Prepared signing payloads retain native f64 values through vector/grouping
composition before MessagePack authentication; JSON transmission has its own
projection. The encoder preserves integer/key ordering and enforces source depth.
Generic array user actions retain indexed/length/prototype reads during signing;
the caller's JSON array cannot expose the two named post-call properties.
`CLAUDE.md#signing-adapter-contract` prices Wallet/error/Promise and snapshot costs.

Preserved source quirks include scientific-exponent zero stripping in floatToWire,
short-vault right-padding and absolute exchange URL retry classification. Fixtures
and historical controls remain intact. Local delivery requires formatting, locked
workspace build and workspace host clippy; central wasm/wiring gates are separate.

The family socket facade maps source `ws.close()` to the shared transport-only
`close_current_websocket()` API. It closes the current native/relay connection
while preserving `forceClosed`, generation and superclass resources. The private
perpetual socket's source `dispose` has no super call; the production shared-Js host
regression checks its wire close and unchanged force/parent-disposal flags against
the actual TypeScript class, including a failing forceClose source mutation.

Hyperliquid Spot uses `spot_binding::HyperliquidSpotExchange::new` over the same
shared family host and native dependency shape. Its registry name is
`Hyperliquidspot`; host callers now pass the selected name to
`RustVenueHost::hyperliquid(name, options)`. Keep the caller-owned limiter shared
with perpetual accounts. Spot preserves its source constructor's perpetual API
factory import and source signing calls, including their default mainnet domain.
Source methods without a Spot override keep BaseExchange's `Not implemented`
error. Candle and order-book listeners invoke the Spot socket class and retain
its cleanup behavior (candle cleanup removes the handler without a wire
unsubscribe).

Spot's `placeOrder`/`updateOrder` return the object produced by actual
`omitUndefined` (`lodash/omitBy`) inside the shared host. The native Exchange ABI returns
`Vec<String>`, so the adapter reads values of enumerable numeric own keys in JS
key order, skips missing indices, preserves string IDs, and spells number IDs
with `js_number_string`. Batch placement preflights every caller ID before the
first order, follows source mapSeries order, and combines the lowered IDs.
It refuses other key/value types and lone-surrogate strings. This is a declared native output choice under
[the typed output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
not an assertion that source `omitUndefined` returns an array.

The Store/DTO boundary uses the shared native typed codec rather than a JSON
roundtrip. Missing Spot `oraclePx`/`funding` fields produce NaN in source and keep
that value through native Store snapshots and lookup tables. Wire JSON still
uses JSON.stringify semantics. Candle calendar arithmetic uses the shared
`js_runtime` dayjs implementation; the host adapter exposes the `subtract` and
`valueOf` operations consumed by this source method.

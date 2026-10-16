# Hyperliquid

The maintained TypeScript under `packages/safe-cex/src/exchanges/hyperliquid`
remains the behavioral reference. The frozen e6a8ffaf13 census is425ported,
106divergent and0pending across531rows, including spot. The current audit is
`sdk-rs/parity/HYPERLIQUID_DIVERGENCE_RESOLUTION.md`; older evidence/claim audits
record their original trees and do not describe the current backlog.

This crate contains typed payloads, action composition, L1/user EIP-712 signing,
order interpretation, pacing, API processing and host-bound exchange/socket
algorithms. Raw class construction and arrow dispatch retain opaque native
receivers over explicit host operations. A row census does not establish a live
browser/venue binding; these tests use source executions and injected hosts.

`create_exchange_api` and `create_info_api` accept an adapter, error emitter and
shared clock-backed `Rc<RateLimiter>`. The caller must supply regional/proxy and
qs-repeat behavior; these factories neither install nor validate it. The shared
`HttpClientOptions.params_serializer` seam exists. Concrete venue binding remains
unproved. See `CLAUDE.md#api-execution-contract` and `#limiter-execution-contract`.
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
perpetual socket's source `dispose` has no super call; the production realm
regression checks its wire close and unchanged force/parent-disposal flags against
the actual TypeScript class, including a failing forceClose source mutation.

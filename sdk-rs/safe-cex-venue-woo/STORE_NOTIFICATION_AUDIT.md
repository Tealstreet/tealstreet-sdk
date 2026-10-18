# WOO Store notification reachability audit

Audited the 534 WOO and journal rows after numeric adoption at `6ba6f181c1`:
87 ported, 20 divergent, 427 pending. The 87 ported rows comprise 45 type/data
declarations and 42 callable declarations. This is a notification reachability
audit, not just a search for direct Store writes.

The initial actual-source AST scan found 17 update-family calls across 11
symbols, all pending: refreshFundingRates, applyOptimisticOneWayPositionFromFill,
start, tick, confirmPositionModeAndRefreshPositions, cancelAlgoOrder, private
handleExecutionReport/handleAlgoExecutionReport, and public ticker/BBO/mark
handlers. There is no WOO setSetting call. Rust WOO has no Store writes; its
order mappers read markets_by_symbol. Those direct scans alone do not establish
the absence of indirect notification through socket reconnect or health paths.

Following the method in Bybit's STORE_NOTIFICATION_AUDIT.md, a second AST scan
resolved all 42 ported callable bodies and enumerated their calls. Manual
transitive follow-through establishes these terminals:

| Source group | Callables | Where the internal paths terminate |
| --- | ---: | --- |
| woo.v3.public.ts | 25 | Local validators and decoders, JS numeric/string/array operations, Object.freeze; sortOrderBook and calcOrderBookTotal mutate only the supplied local book arrays. Neither imported helper calls Store. |
| woo.exchange.ts | 8 | Six arithmetic/price predicates and two response projections, getWooResponseMessage and parseTransactionRowsFromResponse. These bodies call no exchange lifecycle method. |
| woo.order-history.ts | 5 | Local numeric helpers and record construction. The venue's resolveSymbol callback reads marketsBySymbol after normalizeSymbol; normalization performs string operations and memoization, with no notification. |
| woo.utils.ts | 1 | mapWooHedgeTransactionSide performs string normalization only. |
| woo.legacy-fills.ts | 1 | buildWooLegacyFillsParams constructs a query object without an HTTP call. |
| journal/woo.position-adapter.ts | 2 | optionalNum and wooOrdersToExecutions perform local arithmetic and append to a newly allocated execution array. |

No ported callable reaches BaseWebSocket.send/reconnect, close processing,
health publication, snapshot operations, or a core operation that notifies.
All socket lifecycle and registration rows remain pending. The factory and legacy limit/algo
mappers are already divergent, not hidden among these 87 ported rows.

Caller-supplied decoder/resolver effects are outside the fixed internal call
graph, as in Bybit's audit. This does not assert that arbitrary user closures or
JavaScript property getters are effect-free. Likewise, callers invoking a pure
mapper and later writing its result do not make the mapper itself a notifying
operation.

Result: zero affected already-ported rows, including indirect internal paths;
no status changes. Under the subsequent ruling, new handlers may use the current Store ABI, but
every notification-crossing row must be divergent with a routed FINDING and
its call site added here. The full breaking cutover will then force migration. This document records a source audit, not a new
runtime test or a claim that the shared notification seam is repaired.


## Public heartbeat follow-through

The next two callable operations, WooPublicWebsocket.ping and handlePingEvent,
reach BaseWebSocket.send with fixed ping/pong frames. The source classifier
recognizes both before the non-open reconnect branch, so missing/closed sockets
return without reconnecting or publishing health. Disposed parents return before
send; an open host send can throw directly without a fixed notification path.
Forty actual-source cases compare frames, logs, errors and clock ordering and
assert zero reconnects; the Rust caller test also asserts zero health publication.
These add two callable paths with zero notifying writes. They do not clear the
pending ticker handlers' notification contract.

Error-conversion follow-up: inspected every production map_err and Store write
reference after the OKX warning. There are no Store-write error conversions in
this slice; existing conversions concern coercion, Serde and transport failures.
Future StorePatchError conversions must explicitly bind that error type so the
notification ABI cutover cannot silently pass through an inferred closure.

Callee-based follow-up (not only map_err spelling): live limit/algo mapping in
binding/hosts/order_mapping.rs reads the real Store facade marketsBySymbol.
The old typed orders.rs mapper functions have been removed. The other store-crate calls are
sort_order_book and calc_order_book_total on a local book. No Store mutation
result exists to stringify, discard, infer through From/Into, or forward with ?.
The new fixed heartbeat sends terminate before reconnect and health publication
in core. Thus the revised mutation-result conversion inventory is also zero.

## Public writer cutover

The three public writer rows reach one `Store::update_ticker` call in
`update_public_tickers`. It now binds `Result<(), StoreMutationError>` and names
that type in its `map_err`, delegating to core's `ExchangeError::from` rather
than stringifying. Core recovers the original ExchangeError cause and metadata.
The venue neither unwraps nor reconstructs the opaque StoreNotificationError.
The handler signature remains `Result<(), ExchangeError>`.

The existing 87-case production DefaultStore witness now throws through a real
fallible subscriber. It observes the committed first patch, skipped second
subscriber and aborted remaining batch, and a cause code that a string-only
conversion loses. The typed nonnumeric field refusal remains independently
CHOSEN; therefore none of the three rows becomes ported solely from this fix.

The orders fixture's market initialization explicitly expects the Result from
`Store::update`; no subscriber exists and notification is false at that setup
site. The writer fixture's ticker initialization has the same contract.

All six WsParent test hooks return `Result<(), ExchangeError>` and support
injected errors: log, emit, sync_socket_health_to_store, update_latency,
emit_ws_reconnected and emit_and_log_error. None is modeled as infallible.
Their default success preserves the original no-listener fixture behavior;
the hook regression checks exact name, message and metadata on each failure.
Existing log/latency/health observations remain recorded, not erased as stubs.

## Private heartbeat follow-through

Private ping follows core sendWithLog -> send with a fixed heartbeat frame.
Disposal returns before sending; missing/closed sockets terminate at heartbeat
classification before reconnect/health publication. After a successful send or
closed-heartbeat drop, diagnostics check the flag then stop at the heartbeat
classifier. Host send errors propagate before diagnostics. Forty actual-source
cases pin zero reconnects; the Rust fixture pins zero health publication.
The private timestamp helper terminates at local coercion and arithmetic.
These two operations add no notifying Store path; subscribe and private writer
handlers remain pending and are not covered by this result.

The test-linked on_message dispatcher propagates track_last_received with `?`.
Both heartbeat timestamp probes explicitly expect handle_pong_event success;
a separate real-pong test injects a latency-hook failure and checks the exact
ExchangeError. These were additional compiler-collected sites beyond the older
six-hook routing list. No dispatch or lifecycle row is newly claimed ported.

At integration `9419b340d8a6`, canonical conversion retains the original
ExchangeError cause metadata but drops the Store-local subscriber id. This is
a named core structural-retention FINDING on all three writer rows, alongside
their independent CHOSEN typed-field refusal. No subscriber-id preservation is
claimed and no venue sidecar is introduced.

Notification candidate `0134b837a9` validation before the integration merge: scoped lib/tests
check passed; writers (1), public heartbeats (3), private socket (3), and orders
(6) passed. Three compiling controls failed at their named assertions: cause
flattening, swallowed mutation failure, and swallowed parent-hook failure.
All six parent hooks support injected failures; none is intrinsically infallible.

Integration merge inputs: `0134b837a9` and `9419b340d8a6`. Named CI step
multiplicities are 90/90 to 90, with none lost. Real TOML parsing reports
359 exemption blocks / 359 distinct targets, zero missing parent targets and
all eight WOO dependencies retained. Workflow and wasm classification match
the integration parent exactly; WOO source, tests and scripts match the venue
parent exactly. Merged wiring passed 251 assertions; the separate layer audit
through the real Yarn entrypoint passed 78 tests. Counts remain 90/24/420.
No fresh host/wasm Clippy, CI, browser, clean-install or readiness claim.
Merged-tree `cargo check -p safe-cex-venue-woo --lib --tests --locked
--offline --jobs 1` passed after both merge inputs, in 4m29s.

Subscriber-id follow-up against integration `9af82bfeae`: both canonical
conversion branches now append Store-local IDs inner-to-outer. The WOO writer
witness reads the native list and requires one ID for the single notifying
Store. Clearing that list in compiling venue conversion code failed at
`writer retains one Store-local subscriber identity` (0 versus 1), exit 101;
restoring the implementation passed all 87 writer cases. The three ID-loss
FINDINGs are closed, leaving independent CHOSEN typed-field refusals; no row
status changes. No local box or sidecar existed to remove.

Merge inputs for this extension are `3410ecec94` and `9af82bfeae`; step
multiplicities remain 90/90 to 90, exemptions 359 blocks / 359 distinct, and
eight WOO dependencies. Earlier wiring/layer results remain attributed to
the preceding merge, not replayed or relabeled as this extension.
The third post-repair complete lib/tests check (after the candidate and
9419b340d8a6 checks) passed against this extension with zero new diagnostics,
using --locked --offline --jobs 1. WOO-only fmt check also passed. Earlier
compiler iterations had exposed receive-tracking and pong Results beyond
the initial routed list; those were repaired before the three clean passes.

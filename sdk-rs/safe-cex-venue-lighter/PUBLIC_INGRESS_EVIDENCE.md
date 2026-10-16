# Lighter public market ingress

Parent: `a553ce92e78a0559194e953f569bf4fe42e6db8c`. Fresh branch:
`feat/lighter-public-market-ingress`; parent is merged integration work.

Nine pending rows move to ported: both API functions plus public subscribe,
onMessage, toOrderBookLevel, aggregateOrderBookSnapshotLevels,
applyOrderBookLevels, handleTickerEvent and handleMarketStatsEvent.
Only the API/public manifests change. Builder approval remains pending;
exchange/private/signer and the separate SDK crate are outside this partition.

## Native contract

`public_ingress.rs` ports the maintained bodies with mandatory opaque value and
error operations. The host supplies live module bindings, original objects,
property Get/Set/Delete, computed ToPropertyKey, namespace receivers, calls,
constructors, fresh literal DefineDataProperty, callback creation, UTF-16
conversion, strict equality, truthiness, binary operators and iterator protocols.
No default host or JSON value projection implements these operations. Imported
helpers, SDK construction, networking and the timer runtime remain supplied host
operations; this slice does not assemble a native SDK class or make live requests.

The host must implement actual for-of iteration and abrupt IteratorClose; the
boolean iterator callback also supports whole-handler early return. Tuple
binding closes iterators after two values. Constructor throws and thrown Error
values retain identity. Computed keys resolve before assignment RHS. Method
callee lookup precedes argument evaluation and preserves receiver. Globals are
live module bindings; namespace Object/Number/Array and virtualClock calls keep
their original receivers. Subscribe captures each timer handle after scheduling;
a synchronously invoked timer observes the timeoutId temporal dead zone.

Ticker patch assignment and stats literal property definition are distinct.
Snapshot aggregation retains the original first level per exact Map price;
delta zero amounts reach the maintained update-book removal operation. Stats
omit ticker-only mark fallback and PNL updates. Message dispatch snapshots the
handler entries before parsing, preserves callback frame identity, and resets
first-message counters before tracking. Delayed subscribe removes its handle
before observing live disposal/connection/reconnect guards.

## Instrument and scope

Nine named native test groups cover independently authored fixtures. Enumeration
floor is 49 cases, stopping at index 48 (subscribe-stagger-disposed-before-fire).
Each group's independent native assertions run before source capture/comparison.
Assertions inspect decoded native types, UTF-16 values, number bits, fields,
ordered effects, independent literal counts, original argument/result aliases,
constructed Error identity and original opaque failures. No wire roundtrip is a
gate. The source witness transpiles the actual maintained AST initializers and
imports maintained parser/math/order-book helpers; SDK, clock, timers and imported
URL/parse boundaries are dependency spies. The finite native host is not a
complete JavaScript implementation; exotic realms/prototypes/accessors/function
metadata and arbitrary iterator/coercion behavior are contracts, not exhaustive
proof claims.

Read-only sub-agent review corrected namespace receiver handling, canonical
header iteration, interpolation order and whole-handler iterator returns. It
also caught manufactured null/scalar header witness failures and missing
position/constructed Error identity assertions; these were corrected before gates.

## Measurements

Implementation gates will run after integration merge-down and the first push,
as required for this three-lane venue. No Cargo gate or mutation control has
been measured at the implementation commit yet. The extractor writer completed
exit 0 with Updated 0 manifests and no orphaned symbols before commit.

Not run: workspace compilation, full venue/workspace suites, host or wasm
clippy, CI reruns, live exchange requests, SDK class assembly or browser checks.

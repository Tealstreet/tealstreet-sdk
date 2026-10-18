# Native book value aliases

`orderbooks::Level` is exactly `Rc<RefCell<OrderBookOrders>>`; `Levels` is
`Rc<RefCell<Vec<Level>>>`. Both are actual Serde decoders through their concrete
standard-library containers and the shared strict object-only OrderBookOrders
implementation. Neither is a non-decoder callback. The contract records only
these compiler-authenticated native/default types, with no alternate raw
conversion traits. It does not authorize other aliases or a generic Rc family.

`book-level-alias-contracts.mjs` uses the maintained package context and native
probe engine. The initial exact-owner run reports both missing classifications
(and the independent final NumericChanges alias). The two records then admit
complete maps / arrays of complete maps respectively, and refuse primitives,
wrong shapes, missing fields and invalid numeric fields. Direct observations
read all three decoded field bits, including negative zero and distinct values;
there is no JSON round trip of decoded records. Empty collections and ignored
map keys are explicitly admitted. The native tests independently require the
full Rust type via invariant PhantomData, collection order and shared Rc/RefCell
cell identity under cloning/mutation. These alias decoder properties are Rust
admission facts, not invented TypeScript wire requirements.

The production reach is the constructed KrakenExchange -> real assembled public
socket -> live subscription -> physical raw snapshot -> orderbooks::emit_book ->
emitted_levels -> native binding callback. Two Futures and Spot fixtures each
check a nonempty bid and ask collection. The unchanged live TypeScript
emitOrderBook arrows run through the existing maintained orderbook oracle with
`noMarket:true`, matching the production fixture's source `contractSize || 1`.
Every price, amount and total is compared by native f64 bits; a separate assertion
checks both collection lengths and sort directions. The terminal transport is a
fixture; the producer, host, subscription and callback dispatcher are production.

`book-level-alias-controls.py` swaps actual emitted price/amount fields and then
independently truncates actual emitted levels. Each mutation must compile and
fail its named native binding assertion; every exact-byte restoration must pass.
The script restores source even on errors. No production function body changes
are part of this classification PR. These witnesses do not claim shared JS
array identity across callback invocations, a separate behavioral obligation.

NumericChanges remains a separate borrowed-only alias. The general owned
Deserialize, alternate-trait and identity guards are unchanged.

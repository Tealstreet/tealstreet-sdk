# Alternate owner contract cohort

The assigned 33 owners comprise eleven ordinary Deserialize carriers, fourteen
owners with raw-input methods and eight typed constructor/conversion APIs.
The compiler inventory discovers 76 entrypoints across these owners. This batch
excludes the additional lexical `Wire` owners, other Aster cohorts, feature
configurations, aliases and WASM execution.

The eight declared market/position/order/ticker/kline/envelope/placement DTOs
have 435 independently authored field and shape cases checked against maintained
TypeScript declarations. Native projections read every compiler field, nested
placement option and tuple slot, distinguish optional omission from null, and
compare binary64 bits before JSON. Owned, borrowed, text and qualified Deserialize
routes and actual Clone must agree. Positional arrays are refused by strict map
DTOs; the kline tuple requires its twelve declared slots. Raw account/stream
responses continue through their existing production methods, independently of
these declared DTO contracts.

`ExecutionOrder` observes its flattened options and nullable `postOnly` through
owned and borrowed Value decoders and Clone. Direct and qualified text decoders
explicitly refuse these inputs: Serde flatten buffers the arbitrary-precision
number shim before the nested numeric capture. Each receipt records both text
refusals separately. A repository-wide `rg -n ExecutionOrder sdk-rs --glob '*.rs'`
caller audit finds only production native `to_typed` reads at
`binding/members.rs` (single/batch placement), construction and From conversions;
no production caller decodes this carrier from serde_json text. This cohort
preserves that visible refusal rather than claiming four-route equivalence.
`CheckParams` and `ApproveParams` are ordinary parameter Serde carriers: their
union cases explicitly expose optional null and full positional-sequence
acceptance, rather than misclassifying them as strict map-only JS input guards.
Their actual fields and Clone are observed through all four Deserialize routes.

Twenty-two additional source cases execute the maintained public/private socket
classes, real TypeScript DefaultStore and native Map operations against the same
fixed expectations. They cover ticker/mark/trade/depth/kline mutations, private
order/balance/account updates and insertion/deletion ordering.

The fourteen raw owners execute actual SDK methods through the constructed
native Aster binding. Shared fixture code supplies only scripted HTTP/socket
primitives, a deterministic scheduler and a real Store. HTTP config, callbacks,
Map/Error state and Store fields are read directly. The private scope accessor is
injected only beside its owner in the accepted-set sandbox; production APIs and
method bodies remain unchanged. It lets all probes inspect the same binding
scope that the native constructor assembled.

Eight APIs have no raw decoder input: ClockOperation's Future poll, the virtual
clock/realm, private repair and listen-key constructors, prepared-order From
conversions, OrderExecution construction and the generic assembly constructor.
Their exact compiler APIs receive typed classifications. The normal CI
`runProbes` path must emit eight `classificationOnly: true` receipts with no
unclassified panic, separately from the 25 native decoder/runtime receipts.
Constructed witnesses are the real exchange binding target, clock lifecycle,
private repair, listen-key and runtime assembly targets. The new preparation
witness checks every shared/venue option through all three native conversion
routes, including reduceOnly absence/false/true and negative zero.

The selected `scripts/accepted-alternate-contracts.mjs` runner keeps compiler
inventory and exact API validation. Its fixed missing-contract control removes
only these 33 contracts from the in-memory contract map: all 33 actual compiled
owners must then produce an unclassified receipt. That control does not weaken
production discovery or classify neighboring owners.

Verification receipts and compiled production mutation results are recorded
below after the scoped gates finish.

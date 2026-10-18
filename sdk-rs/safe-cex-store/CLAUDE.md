# Store-shaped mirrors

Behavioral specifications live in [safe-cex/CLAUDE.md](../../packages/safe-cex/CLAUDE.md)
and its more specific utility guidance; do not duplicate them here.
[DERIVED_PARITY.md](DERIVED_PARITY.md) records this port's scope, deliberately
preserved defects, representation boundaries, and inverse refusal rationale.
Run package-local Rust tests and strict clippy, plus the workspace parity gates.

The sole production `DefaultStore` now lives in `safe_cex_exchange::native_store`.
This lower crate owns the `Store` protocol, typed patch admission and pure helpers;
it has no dependency on exchange-core. Graph-backed reads and `serialize` return
owned `Result` projections using the preserving-undefined codec. `native_owner`
is required: a native facade refuses an unsupported owner rather than creating
an independent material copy. `BalanceHandle` captures one native referent and
projects only when its fallible `read` is requested. The lower protocol exposes only an opaque
owner capability: it cannot construct a second graph. `StoreReadError` carries
the property/projection phase and original native cause; its diagnostics never
read cause properties. Typed patch admission remains separate from raw native
property mutations, so its error adapters do not replace source throw values.

Default oracle comparisons are frozen snapshots. The ignored `live_snapshots`
gate executes every generator with `--check`; CI runs all targets with
`--include-ignored`, including the in-memory derived gate. See DERIVED_PARITY
for dependency injection, corpus pins, and JSON float-roundtrip requirements.

Order upserts overlay every carried ModelField, including own Undefined, while
Absent retains the old property. The production Store facade compares all 15
fields against the live source; numeric retention is checked before JSON output.

The derived live oracle compares object key sets and recursively checks values;
key iteration order depended on serde_json Map's backing store and is not part
of these domain results. Array order and exact float-bit comparisons remain
load-bearing. The comparator deliberately does not detect object-key reordering.

Changes to `Store` or production `DefaultStore` must also run the facade
composition gate: `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex
--test production_store_wiring`. It constructs the real DefaultStore and passes
it to BaseExchange's constructor. The workspace native CI `cargo test --locked`
also runs this non-ignored test. Package-local store tests alone cannot prove
that the exchange core can consume the production type.

`StoreUpdate` and `StoreUpdateIdsPatch` decode only JSON objects. Their private
serde companions retain field metadata; the shared types shape guard rejects
positional arrays before field decoding. `PackedTickerDelta` intentionally keeps
its source array representation.

Partial model mutation uses `model_patch::PatchSource`. Existing JSON maps can
still supply finite JSON fields; `ModelPatch::set_number` retains native NaN,
infinities and signed zero, and `set_native` replaces a typed nested field without
serializing it. These operations return `StoreMutationError`; its Patch variant retains the original `StorePatchError`, and callers handle the origin explicitly. Known-field validation is staged before committing any model or batch, so an error changes no state, alias, counter or notification. PatchSource is sealed to stable JSON-map and ModelPatch sources.
Malformed nonempty legacy delta indices return StorePatchError rather than panic; empty and info-only deltas remain no-ops before row access. The separate preexisting packed-delta malformed-index and short-value panic findings remain open.
Typed patch admission projects the reached model, validates a staged candidate,
and encodes only the admitted touched fields into the canonical graph. Raw native
facade updates bypass that typed admission and use the same source mutation
algorithm directly, preserving partial writes and property getter/setter order.
Native aliases retain numeric bits even when +0 and -0 compare equal.

The typed patch boundary is CHOSEN: unknown keys are omitted and invalid known
field shapes return errors rather than inhabiting the closed model. TypeScript's
runtime Object.assign admits those values. This choice is disclosed on the
partial-update rows; it is not a claim that Rust cannot represent arbitrary data.

`sequenced_delta_order_book` ports the [sequenced book contract](../../packages/safe-cex/src/utils/CLAUDE.md#sequenced-delta-orderbook-foundation).
Its constructor takes `Arc<Mutex<OrderBook>>`; retain a clone to observe the same
book across snapshots, deltas, recovery and reset. Release book locks before
calling the state machine. Inputs are typed Rust frames (no wire decoder);
sequences distinguish strings, safe-number inputs and arbitrary-precision BigInt.
The live `sequenced_book` integration test runs under the existing store CI gate.
The sequential negative-control runner and measured evidence live in
`tests/sequenced_book_controls.py` and `tests/sequenced_book_evidence.txt`.

Risk calculations return `Result<JsObjectMap<Vec<OrderRiskProfile>>, RiskError>`.
Prototype-key bucket failures use canonical TableHit identities and propagate
at the source access point; no assertion or subset result reaches the caller.
Closed orders and unprocessed IDs remain valid; source symbol membership is a
Set, so position-only inherited names are excluded. The caller must log and
explicitly clear ordersRisk on Err; an empty dictionary means unknown/cleared
risk, unlike a dictionary containing zero-valued profiles. The actual worker
processor remains P6-owned; facade recovery tests use the real DefaultStore.
`format_pnl` also returns Result: Intl checks raw precision bounds before flooring
(NaN, negative fractions and >100 throw); a NaN value returns 0.0 before option
validation, as the source does. Formatting delegates canonical parseFloat.
Returned risk dictionaries retain JS own-key order; crosshair lookup accepts the
Store's IndexMap view and the ordered result via Deref without a wrapper.

`dictionary_write` provides object-valued assignment and nested mutation over
JsObjectMap. Reports separate the reference mechanism from the safe local own
entry. Ordinary replacements and shadows are faithful; missing `__proto__`
whole-object assignments would replace only that map's prototype, while nested
mutations would modify the inherited referent itself. Neither effect is applied
in Rust: literal entries retain the row, and callers must declare the CHOSEN
refusal with a FINDING. Existing own `__proto__` data properties are ordinary
writes, not setter calls. Scalar/falsy-entry and already-replaced-prototype
dictionaries are outside this stock-prototype, object-row adapter contract.

All notifying public Store writes return Result<(), StoreMutationError>.
Subscriber callbacks return Result<(), StoreSubscriberError>; the public-data
view uses that same bottom-layer error type. Ordered fanout stops at its first
Err after the mutation commits, returning an opaque subscriber id plus original
cause. No Store catch, log, callback-error skip or void compatibility path exists.
The caller decides whether to emit and resolve (BinancePM does); an error from
that emitter remains the caller's failure. Unchanged settings do not notify.

Private mutation queues notifications unconditionally, and fanout runs only
after all Store/registry borrows are released. Non-notifying request counts,
auditing and delta consumption do not enter this error path. Legacy immediate
listeners exist only in private unit tests; production uses the single fallible
fanout. The native notification gate covers the actual constructor and PM source
method's eight cases. Core and venue adapter migration remains separate until
its owners consume the breaking API. Packed malformed-data panics are NOT closed.

External Store implementors construct StoreNotificationError::from_subscriber
using their own assigned subscriber ID and the original StoreSubscriberError.
Fields stay private; notification failures must not be flattened into patch or
invalid-operation failures. The original opaque callback cause remains available.

Risk profile field enumeration is observable and is checked separately from the
derived comparator that ignores generic object key order. The actual-source
derived risk corpus covers all three profile key vectors, including the
loss-side limit exception; that branch sets the shared serialization metadata.

Risk value identity has a later cumulative-fill supplement: swapping order PnL
and cumulative realized PnL passes the old corpus but fails its native getter
assertion. Original source-oracle artifacts remain unchanged.

`Store::balance_handle` retains the current balance allocation without a Store
read guard. DefaultStore borrowed balance access and retained handles share
identity; unrelated writes keep the allocation, while balance updates/reset
replace it before notifications and synchronize the owned StoreData projection.
Old handles keep old objects alive. Never recreate the allocation on every read
or clone a snapshot as an identity substitute.

Ticker patches recognize timestamp as an optional numeric runtime field. The
shared type preserves Paradex virtual-clock values; absent values are not replaced
with fabricated clocks. The ordinary patch numeric decoder also applies here.

Ticker alias copies include timestamp bits. Worker fast-path patching retains
the source eleven-field inventory, which does not refresh this runtime extra;
this type addition does not change that source behavior or packed delta fields.
The timestamp target directly pins both id and symbol aliases, including a full
replacement that clears an absent timestamp after numeric patching.

Root utilities: config ports fixed WebSocket constants and exposes the four
module-initialization values as host-evaluated functions (CHOSEN). Supply one
random unit sample and startup environment strings, then retain results for the
runtime lifetime. This does not replace WsRuntime host hooks.

account_environment ports the five pure createExchangeFromAccount environment
helpers; all three required broker fields are read before missing-field refusal,
with per-field fallback to generic Kucoin settings. Account option construction
remains pending.

telemetry holds a per-thread sink, initially empty, and drops its registry borrow
before callbacks; callbacks can replace it reentrantly. Opaque data and failures
retain identity, and Result propagates callback throws. This service does not
wire every venue's existing telemetry/runtime hook automatically.
Declare thread-local state inside the private registry accessor, matching the
other SDK services; module-level item macros are refused by the syntactic index.
report_event is FORCED at the shared UTF-8 name boundary: a sink can read a lone
source unit programmatically, while the required &str argument cannot retain it.
The live root utility oracle witnesses that source callback observation.

Structured WebSocket redaction drops the source fresh-object __proto__ assignment
from own output keys at processed depths. Both the shared utility and core socket
path preserve the depth>12 passthrough; native own-key assertions precede wire checks.

REST diagnostic sealing tracks each modeled property independently; existing
fields remain writable and direct signing preserves partial writes before failure.
Merge readers retain the source repeated reads and eager map-before-dedupe effects.
The focused native regression target is `diagnostics_choices`; its maintained-source
counterpart runs through `parity/scripts/diagnostics-choice-source-gate.cjs`.

Market/Position option metadata participates in known-field partial patches and
native merges. Incoming omission preserves the prior option value; a present
empty PositionOption replaces it. The JS facade uses native ModelPatch carriers
for nested option numbers and forwards Store.update's fromWorker flag, preserving
worker market merge behavior. The facade `option_fields` target compares eight
actual source mutation paths across five numeric profiles; the ordinary production
Store wiring gate remains required for Store changes.

Position upserts overlay all nine ModelField slots by property presence: an
Absent incoming field preserves the previous value, while own Undefined clears
it and Value replaces it. This includes side as well as optional metadata.
Do not use ModelField::or for this merge: its value-only fallback has a separate
meaning. The production JS facade's position_model_merge regression compares
81 outcomes with the maintained TypeScript Store across single, batch and full
upserts; the shared carrier API remains unchanged.

Position patch batches carry `Option<String>` keys across the Store trait and
concrete Store API. `None` means the source selector cannot match a native string
ID, and preserves that row's place in the original batch. Retaining those rows
keeps the source single-versus-indexed lookup threshold and duplicate-ID choice;
no synthetic ID or filtered batch substitutes for an unmatched selector. Typed
venue callers supply `Some(id)`. The native facade's168-case maintained Store
comparison observes selection, duplicate rows and the position update counter.

Order.reduceOnly follows the same ModelField overlay policy: an absent incoming
property retains the existing value, while own undefined replaces it. The native
Store facade regression exercises all four presence states across single/bulk
upsert and updateOrder, alongside the actual TypeScript Store.

Model info patches carry canonical RetainedJsonInput directly through PatchSource.
No JSON copy precedes native Store mutation, and emptiness detection checks carried
StoreUpdate fields without serializing them. Ticker info stripping retains the
source truthiness rule; Order/Position own-undefined merge semantics are unchanged.

Opaque zkLinkConfig uses the same retained carrier through Store.update and its
borrowed getter. The source snapshot still omits this configuration. The core
native Store witness compares thirteen configuration profiles with TypeScript.

Required numeric Ticker patch fields reject own Undefined as a typed patch error.
Candidate publication remains atomic; optional-field and info presence policies
are unchanged. Orderly's real socket census covers per-row refusal and siblings.

Ticker.index patches accept null and retain it across both aliases. Packed ticker
values stay numeric: both maintained packers filter non-numbers. Full ticker
snapshots retain null; packing a null field intentionally omits its delta.

`subscribe_native` is a required opaque native-state subscription. The canonical
owner borrows its actual state referent at each reached slot; lower layers cannot
replace it with a typed snapshot. Internal emitter and native account observers
use it, while `subscribe` keeps its explicit typed projection semantics.

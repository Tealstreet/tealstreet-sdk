# Bitrue venue

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/bitrue/CLAUDE.md).

`types.rs` mirrors configuration exports. Runtime mapping consumers use the
`interval`, `order_type`, `order_status`, and `order_side` lookups, which preserve
inherited identities through shared `TableHit`. Retain the hit until applying the
source branch; a typed own-entry array alone cannot represent inherited values.

`BitrueOrderType` and `FetchOrdersResponseDataItemSide` admit strings before
private Serde remote metadata companions. Their TypeScript literal unions do not
admit externally tagged objects. `accepted_enum_shapes` checks all six native
variants and existing serialization, then rejects tagged maps and other shapes
through owned, borrowed, text and qualified decoders. Run
`scripts/accepted-enum-controls.py` serially: each concrete guard removal must
compile and fail its named tagged-object assertion, and exact restoration must
pass. The two numeric trigger enums retain their separate numeric domains.
The two accepted-set contracts do not classify other Bitrue owners or aliases.
The ordinary inventory still refuses 32 unreviewed trait methods on
`Scope<binding::Extra>`; selected owner receipts do not clear that receiver boundary.

`wire.rs` mirrors declared response shapes, not unchecked Axios runtime values.
Numbers are binary64. Record Deserialize implementations capture `JsNumberInput`
before private map-only companions. Optional fields distinguish omission from
present non-null values. `future_model` and transparent `OpaqueValue.value` are
named passthroughs captured before Value construction. UTF-8 and opaque JSON
boundaries are declared on the corresponding manifest rows.

Oracles require `SAFE_CEX_TYPESCRIPT_PATH`. Configuration/type/utility oracles
execute the complete actual TS module and workspace dependencies without stubs.
The REST-read helper oracle transpiles unchanged actual class-field initializers
and binds their recorded dependency context; it does not claim constructor or
class integration. Quantity tests use the production DefaultStore and distinct
requested symbols, multipliers and missing-market supplements. Type aliases erase at
runtime: `type-schema.cjs` inventories actual declarations, while the runtime
oracle captures native numeric bits before JSON projection. Rust observers
access decoded fields explicitly, independently of Serde field names. Each
oracle uses one Node process for its full corpus. Census controls preserve
inherited required fields and refuse unsupported mixed unions; fixtures give
same-typed fields distinct values and test symmetric Serde-name swaps.

Run the scoped `configuration`, `tables`, and `wire` targets with `--jobs 1`.
`configuration-controls.py` and `scripts/type-controls.py` mutate production
code sequentially and restore after each case. Do not run another compiler or
review mutable source while these controls execute. Full crate gates and separate
host/wasm clippy belong at slice readiness.

`createAPI` has a measured DOUBLE PARSE source defect: cite `f6de9ca8c4`,
Binance GATE_EVIDENCE.md's blast-radius census. Bitrue combines bigInt HOW
customParser with inherited Axios default transformation. Direct and HOW paths
can return different native types for identical quoted JSON. Do not fix TS or
rederive the reproduction. RestResponse currently erases parser provenance; the
REST factory slice must preserve or explicitly mark this gap. No factory
implementation is claimed by the constants/types slice.

Compose subsequent runtime hooks with the shared exchange contract, numeric
ingress, production store, crypto and transport; annotate only Bitrue manifests.

`read_helpers.rs` ports the first five read-helper rows. The coercive helpers
consume the JSON/undefined wire domain, with numeric ingress through shared
OpaqueValue/Market decoding and coercion through safe-cex-num. Timestamp input
is the declared optional Number domain. Callable objects, BigInt and Symbol are
not wire inputs; the manifest discloses this scope and UTF-8 refusal.
`scripts/read-helpers-controls.py` runs six sequential production mutations,
including wrong-symbol selection; each must fail its named native assertion
and pass after restoration. Capture headers record Node version and CPU arch.

Leverage helpers reuse shared ECMAScript whitespace/number parsing, native
ModelPatch and DefaultStore. The oracle executes actual venue and Store field
initializers and observes fields, notification counts and market versions.
Inherited prototype-key hits throw at the same source access point. Bracket
reduction deliberately avoids JS engine argument-expansion stack limits; its
CHOSEN note and live 200000-entry RangeError supplement must remain visible.

Store writes/notifications are fallible. Do not flatten StoreMutationError or
ignore a failed notification: writes commit first, fanout stops at the first
subscriber error, and the original response remains observable. Leverage
witnesses cover that ordering against the actual source notification body.

The leverage setter propagates through shared ExchangeError::from. Its former
box is redundant after structural subscriber IDs landed at 9af82bfeae. IDs
remain ordered inner-to-outer, including equal IDs from different Stores.
Fallback conversion retains message/response, not the opaque cause type.
Independent fallback/recovered empty-ID-list controls must fail native identity
assertions. Account guards retain shared bounded JSON ingress; a separately
labelled depth-160 input documents the source-accepts/Rust-refuses boundary.

Precision audit at d014129251: 0 implemented price-step reads, 0 amount-step
reads, 0 two-axis formatter sites. This is a census; no fields were swapped
and no payload field has a coverage verdict. fetchMarkets/formatCreateOrder
remain pending. Implementing those rows requires an immutable original corpus
(size/hash), labelled live fractional supplements across distinct step pairs,
both sides/order types, and independent native step/payload assertions with a
compiling mutation per applicable output field. Standalone triggerPrice uses
pPrice in source; attached TP/SL currently copy raw options. Preserve that
source distinction instead of adding rounding.

Balance parsing takes a borrow of the production Store balance for the falsy
input fallback. It uses shared JsObjectMap insertion order and math_max, and
coerces only fields explicitly Number-coerced by source. Nonnumeric upnl is a
maintainer-approved per-row refusal: warn with currency, Balance.upnl and its
original value, and preserve valid rows even when the rejected row repeats a coin. The witness inspects native fields,
not serialized Balance bytes; every maximum candidate can independently win.

Cardinality is asserted at source totals/subcorpora, per-case arrays, independent
native lengths, and executed-comparison totals. Read tests/CARDINALITY-EVIDENCE.txt
for the original leverage empty/subset/extra false greens and lookup extra-key
gap. Oracle scripts/captures were preserved byte-identically. Frozen array-count
metadata is a source census, not an independently generated value oracle. Run
cardinality-controls.py and cardinality-production-controls.py sequentially;
the latter temporarily mutates the real Store accessor and restores shared
source after every control. Do not compile concurrently with these scripts.

The consuming-array control is cardinality-consuming-control.py. It consumes
all retained input before returning None, then repeats with both length guards
bypassed. Direct decoded-value equality must still fail. This injects a decoder
defect to test detection; it is not evidence of a live production defect.

Position metadata ports the complete async source initializer with account-fetch
and disposal dependencies supplied at its source boundary. Cache replacement is
atomic after numeric coercions, even when disposal suppresses returned records.
Empty supplied snapshots clear the cache; missing snapshots preserve it. String
positionType "2" is CROSS (the source comparison is strict). Duplicate symbols
replace their original Map slot and duplicate requested symbols repeat output.
The position_metadata witness executes actual initializers in one Node corpus;
its source counts, native lengths and record-comparison total are independent.
Run scripts/position-metadata-controls.py sequentially and restore before gates.

Metadata results share Rc<RefCell<PositionMetadata>> handles with the cache.
Duplicate requests alias the same object; replacement detaches old handles
without mutating them. The live identity supplement mutates a returned handle
and checks the duplicate/cache, then verifies detached handles after refresh.
Cache entries store their immutable Map key separately from the mutable record.
Mutating a returned symbol must not cause a fetch or change lookup by old key.

Leverage reads compose the existing source-matched bracket parser and fallible
Store setter. The outer fetchAndSync source catch deliberately suppresses every
failure, including notification failure after commit; its TS-BUG comment names
that discard. Do not suppress errors in the shared setter. Public fallback reads
the post-await Store maximum, so a caught subscriber failure can still yield the
committed maximum. GET URL/config are produced by the implementation and
observed natively, while API factory/signing integration remains pending. The
200000-bracket CHOSEN supplement records the inherited iterative-max difference
through all three wrappers, including their Store effect, not just the scalar.

Account reads preserve the raw GET/catch/emission sequence. The catch does not
catch its own emitAndLogError failure: the shared error (including subscriber
IDs) propagates. The balance wrapper awaits the injected account hook before
borrowing the current Store balance; falsy data preserves that native reference.
The Store's Rust borrow lifetime is a declared restriction on JS mutable aliases.
account_reads runs one actual-source Node corpus with fixed independent inputs
and live per-case content checks. account-reads-controls.py checks production
mutations, same-length replacement/reversal, and a native failure with source
length/content admission removed. No transport factory integration is claimed.

`fetch_balance` returns a retained Balance allocation after the awaited account
fetch. Falsy data retains the Store current balance handle; parsed data gets its
own allocation. The result holds no Store Ref guard and survives ticker writes,
balance replacement and reset with the old object intact. The unchanged
`tests/balance_retention.rs` regression must panic on the original guard-based
path and pass after repair; pointer identity is part of its native assertions.


Stateless WebSocket slice: ws_private.rs ports isSuccessfulCode and ping;
ws_public.rs ports ping. SuccessCode preserves numbers and UTF-16 strings;
all other JS values are strictly comparison-equivalent false without coercion.
These are field-body ports; WebSocket construction, transport and callers remain
pending. ws_primitives.rs executes actual source initializers and asserts native
booleans/unit before inventory checks; one runtime discriminator per row is required.

`binding::BitrueExchange` now assembles those existing field bodies on the shared
JS runtime with production Store, signed HTTP client and both BaseWebSocket
cores. Its Exchange implementation forwards all sixteen required methods; the
five methods absent from the source class retain `base.ts`'s exact inherited
`Not implemented` rejection (getAccount, fetchTickers, changePositionMode,
scheduleCancel, fetchPublicTrades). changeMarginType keeps Bitrue's explicit
source refusal. Batch placement receives a ValidatedBatch; single placement
receives a ValidatedOrder, enforcing the shared client-ID gate before dispatch.

The binding keeps the existing typed/JSON helper boundaries and their recorded
divergences. The native Exchange boundary also converts declared typed inputs
and results through JSON; executable callbacks and socket/class handles stay in
shared JS values internally. HTTP creation uses the shared source factory and runtime provider. The source
tealstreetTunnel branch installs its BigInt adapter through the shared factory;
proxy authorization follows venue signing. No venue-local adapter refusal remains.
Public jsonParse performs ordinary Number parsing, while the private big-int
parser retains its source non-string passthrough. Browser compressed Blob input
uses the existing Store read_blob helper with its byte-to-character projection.

`tests/exchange_binding.rs` covers all sixteen methods, caller-ID fail-closed
behavior before any single/batch write, private listen-key failure without a
private dial/subscription, and the native Buffer/plain-JSON public frame path.
`scripts/binding-controls.py` sequentially breaks each trait dispatch and requires
its named assertion, then a restored pass. Do not compile concurrently with it.
The registry's native_host_bitrue test verifies the HMAC at a local HTTP server.

`routing-controls.py` compiles disconnected factory and adapter bindings; both
must fail the constructed venue routing assertion, then pass after restoration.
Run these controls sequentially without another compiler.

The typed `Exchange::update_symbol_metadata` entry forwards to the assembled
`updateSymbolMetadata` body. It fetches the symbol's leverage bracket and commits
the returned maximum to the Store, preserving the source public-only guard and
caught-fetch behavior. The real-binding regression in `tests/exchange_binding.rs`
checks both the HTTP request and replacement of the stored leverage limit.
The same test target drives a real private socket frame for each of orders,
fills, positions and balance. Account/balance notifications exercise the source
350ms refresh through the signed REST binding before checking the Store.


The production collection host consumes `ReadHost::items` with the shared iterator
protocol. The background order loop reads the actual `prioritySymbols` Set before
issuing requests; requiring an array there aborts every symbol refresh. The
constructed-binding `background_orders` target checks known LIMIT rows alongside
IOC/FOK/POST_ONLY/STOP_LIMIT rows, real HMACs and asynchronous Store commits.
Those REST type lookups preserve own undefined through shared ModelField rather
than dropping the symbol’s snapshot. The private parser separately maps these
native types to Limit; `exchange_binding` checks that source behavior on actual
private frames. REST and WebSocket lookup rules must remain distinct.

REST signing uses `PrivateHost::virtual_now`, bound to the shared synced clock,
matching the API's `virtualClock.getCurrentTimeMs()`. `PrivateHost::now` remains
raw for the private order-event `Date.now()` fallback. Registry
`native_host_bitrue_clock` proves the distinction with a 90-second server offset
and independently verifies the HMAC on a real native signed account request.

Public depth rows use the shared iterator protocol for the source two-entry
array pattern, including early close after two entries. Numeric-keyed objects
and scalar rows must not be published as books. The constructed raw-socket
regression compares actual source refusals and verifies a later valid snapshot
still reaches its callback; it does not claim an async rejection diagnostic sink.


`exchange_binding::production_ticker_coercion_matches_constructed_typescript` exercises
32 field-independent cases through the constructed venue's actual socket and
DefaultStore. Null, numeric/prefix/NaN strings, objects, overflow, signed zero and
false arrive beside good rows as consecutive single-row protocol frames. The
maintained TS exchange and actual TS DefaultStore produce the comparison values;
eleven native number fields are checked before JSON. The shared ticker census
records the witness; `parity/scripts/ticker-native-controls.py bitrue` compiles
a production binding fault, requires the named assertion, restores and reruns.

The constructed public candle witness compares all six native fields against
maintained `mapKline` with installed Day.js, including Singapore wall-clock time,
subsecond precision, epoch zero, invalid-date NaN and signed zero. Both the real
socket witness and an independent installed-library probe fail on master without
`utcOffset` and pass with the shared #453 implementation. Callback cleanup is
observed; typed Candle still has its separately declared retained-object limits.

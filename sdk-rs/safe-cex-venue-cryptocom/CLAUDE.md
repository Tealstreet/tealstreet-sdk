# Crypto.com Rust venue

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/cryptocom/CLAUDE.md).

Production binding work lives in `binding/exchange.rs`: `CryptocomExchange::new`
assembles the Rust exchange/API/socket bodies on `safe-cex-exchange::js_runtime`
from real options, a Store, HTTP and WebSocket runtime dependencies. The Exchange
trait forwards the supported methods and reproduces inherited source rejections
for `fetchSymbolMaxLeverage`, `scheduleCancel` and `fetchPublicTrades`;
`fetchHistoricalFills` returns the source's empty list. Registry wiring belongs
to the registry lane. `safeCexAxiosCreate` goes through the shared
`client::factory::create` with the runtime's transport provider, so a proxied
account takes the provider's private-proxy baseURL and proxy authorization after
venue signing. `installTransportAdapter` installs the provider's default adapter.
`create_api_routes_through_the_provider_and_tunnel_adapter` covers both, and two
`scripts/binding-controls.py` controls disconnect each.

Private data coverage in `tests/exchange_binding.rs` drives code-zero frames
through the authenticated production `/user` socket: batched regular/advanced
orders, partial/full fills and cancellations, hedged positions and zero-quantity
closure, and balance/currency replacement plus empty-update handling. Expected
values follow `cryptocom.ws-private.ts`; the order handler updates Store orders.
`scripts/private-frame-controls.py` disconnects each production topic handler,
requires its named test to fail at runtime, then restores exact bytes and requires
a pass. Run these controls alone because they temporarily edit production source.

Disposal follows the source flag write and public/private socket teardown.
The shared writable `isDisposed` accessor updates the typed base flag without
calling full BaseExchange.dispose, preserving the source's inherited listeners. `tests/exchange_binding.rs`
holds an HTTP response across disposal and checks that no market update, ticker
request or socket construction follows. It also checks read purity, public-only
startup and private subscription ordering across failed/successful auth acks.
`scripts/binding-controls.py` runs compiling production mutations sequentially,
restoring exact source bytes and requiring each named test to pass again.
The venue's rejection wrapper remains necessary: the shared HTTP client's retry
loop surrounds the complete response chain, while axios-retry runs before the
venue's rejection formatter. Delaying rejection formatting preserves the HTTP
response/config used to identify a signature rejection. Removing the wrapper
must fail the non-2xx signature-resend E2E test until the shared ordering is fixed.

`cancelSymbolOrders` and `cancelAllOrders` forward to the source bodies, which post
the venue `private/cancel-all-orders`; the base store-only sweep would miss
orders the Store has not mapped. Both rethrow a venue rejection.

`fetchOHLCV`, `listenOHLCV` and `listenOrderBook` forward to the source bodies
(GET `public/get-candlestick`, `candlestick.<interval>.<symbol>`, `book.<symbol>.50`);
the base defaults returned `[]` and never subscribed.

Source REST `fetchBalance` returns currency records with `available`/`locked`
under an `as any` cast. The binding uses shared `CurrencyBalance::AvailableLocked`
at both validation and Store boundaries, preserving those exact fields. It does
not normalize them to free/used/total/upnl. Native E2E checks USD available9/locked6
from quantity15 and max_withdrawal_balance9, including exact serialized keys.
Run `parity:check` against the actual workspace; the historical one-crate source
view lacks workspace exemption and structural re-export context.

The first slice mirrors the eight configuration constants and
`mapCandlestickInterval` from `cryptocom.types.ts`. Eleven wire records (Instrument, Ticker, Candlestick, PositionBalance, Error, Trade, AccountsResponse, BalanceSummary, Book, TradeFill and Order)
are now declared in `wire.rs`; other records, REST and websocket dispatch remain
pending. Signing/auth helpers are mapped separately below. Decode wire JSON with `safe_cex_num::numeric_ingress::js_from_str`, never
through a Value buffer. Explicit exhaustive output guards register every field.
Private remote Serde companions use the shared object-shape guard.

`map_candlestick_interval` uses the shared ordinary-object `TableHit`, retaining
inherited values rather than silently turning the TypeScript table into a closed
match. Callers must inspect the returned identity before consuming a string.
The shared UTF-8 boundary is declared only in
[MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary).
Constants preserve source order; endpoint groups reference the endpoint table.

`tests/types_oracle.cjs` transpiles and executes the entire actual TypeScript
module once per test process through a shared `OnceLock<Result<...>>`; failures
are retained too, so concurrent tests cannot retry a failed subprocess. Each
mutation runs a new test process and therefore a fresh corpus. It captures threshold typeof/bits and inherited interval identity before
JSON serialization. The corpus records the Node version, Node major, CPU
architecture and platform; the Rust witness logs these alongside its architecture.
Only NaN payloads are canonicalized; signed zero remains bit-exact. Its runtime export census covers this configuration slice;
the wire corpus uses the real TypeScript compiler to resolve 52 fields across
six flat interfaces plus the two nested AccountsResponse properties. Inherited required fields have a permanent positive control;
unresolved ancestry, declaration merging (including inherited parents), and
unsupported member/type shapes fail closed. Native-field
assertions bypass serde roundtrips, distinguish every string/number field and
use one-hot boolean cases. Type declarations have no executable emitted body;
compiler shape evidence is distinct from function behavior. No REST/WS consumer
is claimed.
`tests/controls.py` mutates only this crate and briefly the TypeScript ping
constant, restoring exact bytes in finally blocks. Run it alone, not concurrently
with other commands that edit or test these files. No frozen/shared feeder is used.

During porting use `cargo check -p safe-cex-venue-cryptocom --jobs 1` or the single
`--test types` target. Run only one Cargo invocation at a time. The full scoped
suite and both strict Clippy targets below are merge-up readiness gates.

Scoped gates (from repository root, with
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`):

- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-cryptocom --jobs 1`
- `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-cryptocom --check`
- `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-cryptocom --all-targets --jobs 1 -- -D warnings`
- `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-cryptocom --target wasm32-unknown-unknown --all-targets --jobs 1 -- -D warnings`
- `python3.12 sdk-rs/safe-cex-venue-cryptocom/tests/controls.py`
- `python3.12 sdk-rs/safe-cex-venue-cryptocom/tests/wire_controls.py` (or name a single control during porting)
- `python3.12 sdk-rs/safe-cex-venue-cryptocom/tests/process_count.py` (requires one Node invocation for the three tests)
- `python3.12 sdk-rs/safe-cex-venue-cryptocom/tests/parity_scope.py` (canonical indexer, one-member source view, six venue manifests, no exemptions)

Cargo on this host is `/opt/homebrew/opt/rustup/bin/cargo`.

Configuration-slice evidence: the two live Rust tests compare nine runtime
exports, 36 endpoint entries and 31 interval inputs (10 own mappings, 12 inherited
identities, nine errors). All 13 mutation controls failed at their named assertion
and passed after byte restoration, including `constant_bits:CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS`
for TS negative zero versus Rust positive zero. Scoped native tests, format,
host/wasm strict Clippy and the canonical parity gate passed; the latter indexed
137 rows and nine public Rust items with no exemptions. `tomllib` measured
8 ported / 1 divergent / 128 pending at the configuration commit. Later wire
slice evidence is separate; signing, REST, socket behavior and full TypeScript
extractor validation remain unclaimed.

The first wire slice adds five records / 45 fields. `live_wire_records` checks
native fields, numeric bits, required-field removal, wrong scalar kinds, populated
positional-array refusal and the explicit unknown-key/UTF-8 choices. Seven
production controls and four census controls observed named failures and
restored passes. The process-count gate measured one Node invocation for all
three tests; cached failures also remain within that invocation. The scoped
parity indexer measured 137 rows / 14 public items, with tomllib status counts
8 ported / 6 divergent / 123 pending. This is type evidence; no REST or websocket
callers have been implemented or classified as repaired.

At this wire-slice boundary, the scoped full native suite passed three tests,
and strict host and wasm Clippy each exited zero in separate sequential runs
with `--jobs 1`. All fourteen mapped rows were re-audited through the shared
live corpus, with direct decoded observations for the five new records.

Local follow-on slice: CryptocomTrade preserves five numeric fields through
shared numeric ingress and represents the inline BUY/SELL union with
CryptocomTradeSide. The compiler-derived corpus enumerates both literals;
assertions match decoded native variants independently of Serde names. The
trade mutation swaps price/quantity names and a second control symmetrically
swaps BUY/SELL serialization and deserialization names. An exact compiler literal
set assertion also detects source narrowing; its control removes SELL and
requires trade_side_literal_census to fail. Existing source numeric
strings remain strings; this trade row follows its actual numeric declarations.
Reviewed remote heads stay pinned until merge or release; follow-on work must not
advance an active reviewed head.

The next local type is CryptocomAccountsResponse, with CryptocomAccountIdentity
representing its inline this_account object. The shared oracle resolves nested
compiler properties and compares the exact shape. Native uuid observations are
independent of Serde output names. Both levels refuse populated arrays, missing
required fields and invalid scalar kinds; unknown keys at both levels follow the
explicit CHOSEN omission. Three production controls cover uuid corruption and
outer/inner object guards; a census control drops the nested compiler fields.
No Store mutation, subscriber handling or REST account fetch is implemented.

CryptocomBalanceSummary is local follow-on work: ten string fields plus the
position_balances array reuse the existing PositionBalance record. The actual
compiler resolves the referenced interface; unsupported/unresolved/merged array
element declarations and recursive schema references fail closed. Zero, one and
two entries cover cardinality, ordering and distinct per-entry field values.
Native observations cover every field; totals-swap and entry-collapse controls
must fail independently of reserialization. Object-guard and array-census
controls protect shape admission and compiler coverage. Unknown properties and
unpaired UTF-16 surrogates retain the declared CHOSEN boundaries at both levels.

CryptocomBook adds a nested data object and ordered three-string bid/ask tuples.
The compiler census checks both tuple arity and member kinds. The corpus captures
native numeric bits at depth and two timestamps before serialization, retaining
signed zero and canonicalizing only NaN. Tuple strings and bid/ask sides differ,
including different side lengths; empty sides and zero/one/two data rows are
covered. New controls swap sides/tuple positions, remove both object guards,
corrupt depth and blind the tuple census. Full readiness gates attach to the exact reviewed slice SHA; later slices
start from fetched integration after the previous head is released.

Precision audit at 710dd2b45c4a: VALID NEGATIVE. Three Rust source files contain
zero derived price/amount precision reads and zero order/bracket/trigger
formatters; tick sizes are only wire fields. fetchMarkets, placeOrder,
placeOrders, updateOrder and mapStopTriggerType remain pending. This is no claim
of swap immunity. Before implementing two-axis formatters, add live fractional
cases across several distinct price/amount step pairs, both sides, limit and
stop-market, with direct native payload assertions. Production precision swaps
must compile, fail their named assertions with exit 101, then restore and pass.
Do not repair frozen fixture values to create discrimination.

CryptocomTradeFill adds 19 fields, including seven numbers, four literal-union
fields and optional client_oid. A missing client_oid is None and omitted on
output; a present field decodes through String and rejects null. The compiler
runs with strictNullChecks and describes declared property types so explicit
nullable unions cannot collapse into strings. Exact optional/literal inventories
catch source narrowing. Live cases distinguish side and taker_side, absent and
empty client IDs; native enum matches catch symmetric liquidity serde renames.
Four production controls and three source census controls cover the new row.
No precision formatting or Store call sites are introduced by these wire types.

Cardinality audit: baseline production controls fully deserialize input before
clearing or shortening a decoded vector. They do not rely on Serde trailing-input
errors. All four implemented Vec sites (summary positions, book data, bids and
asks) already failed complete native-value comparison for empty/subset mutations.
This is measured coverage of those sites, not a claim about pending REST/WS lists.
The baseline harness did have three false greens: empty interval cases, empty
account cases and TradeFill cases reduced to its base case.

The cardinality repair admitted 211 cases plus eight constants; the Order slice
adds 89 cases without changing any earlier payload values. A checked-in inventory
contains case labels only, never payload values. Raw-input collection sizes are
captured separately from comparison projections and checked against fixed
zero/one/two-entry and bid/ask population profiles. Native and projected lengths
are independently checked against those raw-input counts; executed comparisons
are counted against fixed expectations. Auxiliary extra-property and surrogate
checks have explicit cardinality requirements. Existing corpus inputs and observed
payloads remain unchanged; only sourceCardinality metadata is added to the oracle.

Run tests/cardinality_controls.py sequentially with the same TypeScript compiler
environment as other controls. It includes consuming empty/subset production
controls, source-case truncation/duplication, population shifts and independent
projection truncation. For every Vec site, paired controls disable both length
checks while retaining the consuming mutant (full-value comparison must fail),
or disable full-value comparison (native length must fail). Joint native and
projection truncation must still fail against raw-input cardinality. These are
injected defects used to test the witness, not claims of live decoder defects.

CryptocomOrder mirrors 25 source fields, including 18 optionals, four optional
numeric timestamps, literal unions and optional string-array exec_inst. Missing
fields remain distinct from present empty strings, false, zero and empty arrays;
present null is rejected through a typed generic present-value deserializer.
Legacy time_created/time_updated fields remain independent of create_time and
update_time. OrderType, TimeInForce, RefPriceType and OrderStatus extract inline
unions; side reuses TradeSide. Native literal matches detect symmetric serde
renames. Price/quantity values remain source-declared strings; no formatting or
precision arithmetic is implemented.

The Order supplement has 89 cases and exact optional/literal inventories. Raw
input cardinality pins exec_inst absence and zero/one/two/three entries separately
from projections and native lengths; the witness also asserts 89 executed case
comparisons. tests/order_controls.py covers native string/timestamp field swaps,
four symmetric enum renames, null/object admission, source schema narrowing,
consuming array mutations, independently disabled length/value checks, joint
projection/native truncation, missing cases and zero executed comparisons.
The original 211-case corpus remains byte-equivalent in its input and observation
values; new Order cases and cardinality metadata are additive.


Utility slice: `signing::params_to_string` mirrors the actual private
`paramsToString` body with shared String coercion, UTF-16 key sorting and the
source's level>=3 cutoff before other branches. Signature/auth helpers have the
separate live witness below. The JSON-data boundary and rationale are declared once in
[MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#cryptocom-signing-data-domain).

`tests/signing_oracle.cjs` evaluates the unchanged complete TypeScript module
and exposes only its private serializer. HMAC/clock imports throw if invoked;
this slice makes no authentication claim. One Node invocation returns 280 cases
(28 distinct inputs x 10 levels), plus separately observed excluded-domain
behavior. `tests/signing.rs` asserts source size, every case/level identity,
native string or error and executed comparison count. Raw input numbers use
whole-entry numeric ingress with the explicitly retained `Input.value` passthrough.
The existing types corpus is unchanged. Run `tests/signing_controls.py` for
cutoff/depth/key-order, empty/subset output and source/loop cardinality controls;
run `tests/process_count.py signing` to measure this corpus's one Node invocation.
No new collection decoder or precision formatter is introduced by this utility.


Signature/auth helpers use the shared local `safe-cex-crypto::hmac_sha256`
backend synchronously at invocation, matching the source, with shared JavaScript
Number-to-String and VirtualClock. Requests retain the caller's captured
SigningValue params identity; this slice does not claim Axios/JSON serialization.
Clock reads precede signing; explicit nonces (including zero and NaN) avoid the
second read. Source TypeError name/message are preserved in ExchangeError. See
the current signing execution boundary in MANIFEST-SPEC; HTTP and socket
dispatch remain separate obligations.

The auth witness runs the actual source and real create-hmac package in one Node
invocation, with an injected clock sequence and pre-serialization number bits.
Only NaN is canonicalized. Its independent input inventory pins all method,
credential, parameter, numeric-token, branch and clock contents for 42 cases (the original 36 plus six error-path supplements).
Native assertions cover every output field, params reference identity, per-branch
clock counts and error name/message before source field-list checks. The original
280 serializer payloads remain unchanged; their raw input contents are now checked
live against a separate inventory. Same-length replacements and reversal controls
protect admission in both targets; archived hashes are not used as parity gates.

Run `tests/auth_controls.py` and `tests/signing_controls.py` sequentially with
the compiler environment above. A control must compile, fail its named runtime
assertion, and pass after exact byte restoration; compilation failure alone is
not discrimination. Auth function rows map their native inline-result structs as
explained splits and use the canonical CHOSEN signing boundaries.

`api::is_public_endpoint` and `api::is_non_retryable_private_write` mirror the
source substring classifiers, including endpoint text inside longer paths and
query strings. None supplies the private-write helper's omitted URL default.
The endpoint witness executes both complete TS modules and calls the actual
private helpers; transport imports are uncalled stubs. It compares native bools
over 29 independently admitted inputs, then requires 58 executed comparisons.
The capture's null input marker represents omission, not a JS null argument.
Run `tests/api_endpoint_controls.py` alone for one compiling substring-to-equality
mutation per helper and exact restored passes. `createAPI`, retry callbacks and
HTTP/socket dispatch remain pending.

`order_mappings` extracts six stateless CryptocomExchange switch bodies.
Borrowed UTF-16 units preserve unknown strings and lone surrogates; None represents
the comparison-equivalent non-string fallback, because strict string comparison
never invokes argument coercion. Outputs use existing finite native enums. The
live witness executes unchanged complete source modules and actual methods with
constructor-only dependency stubs; construction and consumers are not ported.
It pins source enum inventories and 161 independently admitted native comparisons.
Run `tests/order_mapping_controls.py` sequentially for one compiling wrong-result
mutation per new row, named runtime failures and exact restored passes.


Private order mapping slice: private_order_mappings.rs mirrors the three
CryptocomPrivateWebSocket switch bodies (type, status, time in force), separate
from the exchange mappings. UTF-16 inputs preserve strings; None models the
strictly comparison-equivalent non-string default without coercion. Constructors
and consumers remain pending. private_order_mappings.rs witness asserts native
enum variants before inventories; each row requires a runtime discriminator.

Stateless WebSocket policy slice: ws_private.rs and ws_public.rs each port
shouldEnforcePongMissReconnect, getWatchdogStaleThresholdMs,
getWatchdogPingThresholdMs and the empty ping body. Thresholds reuse types.rs
constants. Constructors, transport, scheduling and callers remain pending.
ws_policy.rs executes unchanged actual initializers and constants, asserting
native booleans/f64/unit before inventory checks; each row requires a runtime control.

Socket body slice: socket_handlers.rs now owns public/private message routing,
ticker/book throttle initializers, the trade no-op, private order/position/balance
updates, and public OHLCV/book listeners. socket_classes.rs supplies ordered field
construction and concrete new body closures; existing_member bridges to older
native typed hosts remain a class-level FINDING. SOCKET_EVIDENCE.md documents the
mandatory opaque language/host contracts and the scoped 60-case witness.

The production dispose override delegates to the source body. Shared js_runtime
`isDisposed` is writable and updates the typed base flag, so no full base dispose
call is needed. This preserves the source's inherited event listeners while
disposing public then private sockets. The binding lifecycle regression checks
the flag and retained update listener; shared js_runtime's live-source disposal
oracle checks ordering and abrupt completion.

The accepted-contract string slice reviews seven concrete wire enums already
using String admission and private Serde remote companions. Direct native
variant/serialization tests, seven compiling tagged-map mutation controls and a
closed fresh-rustdoc production probe selector accompany the per-crate shard.
See STRING_CONTRACT_REVIEW.md and scripts/string-contracts.mjs. Scope is these
seven original native/default census owners; the other owners, aliases,
construction records, locals, features and target execution remain separate
obligations. No decoder behavior or manifest classification is changed.

Native/default accepted-contract review for six concrete scalar wire records and
five finite carrier enum APIs is documented in RECORD_CARRIER_CONTRACT_REVIEW.md.
The closed selector is scripts/record-carrier-contracts.mjs; native field/variant
observations and mutation controls run independently of compiler shape discovery.
This is eleven original census owners and six specific Clone reviews; other
owners, aliases, construction entries, locals, features and WASM execution remain
separate obligations.

The nested wire accepted-contract slice covers seven remaining concrete wire
records and two private presence decoders. See NESTED_WIRE_CONTRACT_REVIEW.md and
scripts/nested-wire-contracts.mjs for scope, native observations, exact selected
construction reviews and compiling mutation controls. Other owners, aliases,
locals, features and WASM contract execution remain separate obligations.

Eight runtime helpers have individual accepted-set role reviews accompanied by
direct private-function witnesses and compiling corruption controls. See
RUNTIME_HELPER_CONTRACT_REVIEW.md and scripts/runtime-helper-contracts.mjs. These
forwarding, dispatch and outbound rendering/state-write roles do not waive any
downstream decoder, alias, construction, local or target-matrix obligation.


Accepted signing roles: five individually reviewed native/default functions
serialize existing data or construct outbound signatures/request fields.
SIGNING_ROLE_CONTRACT_REVIEW.md separates fresh compiler classification receipts
from the actual live native signing/auth targets and per-body mutation controls.
SigningValue admission, other owners and the existing domain boundaries remain
independent obligations; no production behavior or manifest status is changed.


The websocket accepted-contract slice separately reviews seventeen captured
native field carriers and the private field guard/borrow helper. See
WS_RECORD_CONTRACT_REVIEW.md and scripts/ws-record-contracts.mjs for independent
field/presence/bit/cardinality cases and mutation controls. This is the declared
static projection boundary, with no JSON/property binding or live alias claim.
Other owners, aliases, construction and full discovery/matrix remain owed.

The seventeen-owner helper batch reviews three enum-returning mapping decoders and
fourteen specific comparison/rendering/dispatch/state roles. Every owner has a
direct native witness and individual compiling body control; compiler
classification receipts remain separate from executed evidence. See
HELPER_BATCH_CONTRACT_REVIEW.md and scripts/helper-batch-contracts.mjs. Supplied
host parsers, generic Clone behavior, downstream decoders, aliases and the full
discovery/feature/target matrix remain independent obligations.

Fifteen control-flow/property/adapter owners have individual accepted-contract
role reviews backed by three direct native witnesses and twelve helpers reached
through unchanged actual-source native caller tests. All fifteen body corruptions
and the retained counted fixture walks require compiled named failures and restored
passes. See CONTROL_FLOW_CONTRACT_REVIEW.md and scripts/control-flow-contracts.mjs.
The fifteen compiler classification receipts remain separate from behavioral
proof; downstream admission, parser grammar, generic hosts, aliases, construction
and full discovery/matrix obligations remain independent.

The test-only account_admin, account_envelopes, private_lifecycle and
public_lifecycle modules and their fixtures/oracles were removed after the
wave2-vacuous-tests audit found no production callers. Their accepted-contract
entries and selectors were removed with them; manifest rows now point at the
production class and socket bodies. Retained historical logs are archives, not
coverage claims. Production lifecycle and admin coverage lives in
`tests/exchange_binding.rs`; streams/history and private data frames are separate
follow-up obligations.

The binding’s REST and private WebSocket auth helpers read the shared synchronized
VirtualClock directly. Do not reconstruct a clock from one raw runtime reading.
The isolated native_clock target syncs the actual singleton, checks emitted IDs,
nonces and HMACs, authenticates the private socket and maps a private order frame
whose missing update timestamp falls back to the same synchronized clock.

The concrete binding's spread and IteratorRecord slots consume the shared native
GetIterator protocol. Spread keeps astral code points and lone UTF-16 units;
for-of caches next once and pulls between source body calls. Body throws perform
IteratorClose without replacing their original cause; next/done/value errors do
not close. Pair destructuring closes early and validates the return result.
`binding::exchange::iterable_tests` reuses the real constructor/transport harness
and connected public socket; it checks callback identity, emitted subscription
channels, native string units, pull ordering and cleanup. These iterator-valued
inputs exercise latent language contracts; maintained wire inputs remain arrays.

The SocketOps pair reader used by book listeners now reuses the existing
IteratorRecord helpers. Numeric-key objects from real JSON frames are refused
before the book callback, matching source array destructuring; the error remains
visible and a later valid array frame still reaches the listener.

The constructed ticker census in `tests/exchange_binding.rs` drives 96 cases
through 288 real ticker, book and valuation frames, comparing every native
Ticker number with the constructed TS exchange and DefaultStore. Source uses
fresh exchanges per frame to isolate numeric mapping from throttle timing;
native frames advance the terminal clock beyond the throttle window. Index
fixtures retain the actual underlyingSymbol lookup. No throttle timing parity
is claimed. The finite oracle exits after flushing captured output because the
maintained disposal does not release all timer handles. A live last/bid mutation
must compile and fail, then pass after exact source restoration.

`native_bindings::Prepared<V>` has one reviewed retained-input tuple instantiation
for accepted-set decoder coverage. It remains the live method/receiver pair in
BoundHost; native constructor-backed tests prove original referents, getter order,
UTF-16, own undefined and exact numeric bits. Run `scripts/public-alias-contracts.mjs`
from sdk-rs for the finite guarded inventory; `scripts/prepared-alias-controls.py`
compiles pair-order and receiver mutations and requires exact restored passes.

Socket order-book sorting uses canonical V8 TimSort with the source subtraction
comparator, preserving signed-zero ties and NaN ordering. The two socket-book
sort tests in `exchange_binding` compare 13 maintained-source profiles per side
through the real socket and typed callback; see `BOOK_SORT_REVIEW.md`.

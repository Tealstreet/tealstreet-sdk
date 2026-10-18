# Backpack venue port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/backpack/CLAUDE.md).

`src/constants.rs` mirrors the six exported tables from `backpack.types.ts` in
source property order. Grafana lists reference the endpoint table entries.
The TypeScript module is transpiled and executed fresh in the mandatory
`tests/constants.rs` witness; each table compares every key/value and its order.
The oracle requires `SAFE_CEX_TYPESCRIPT_PATH`, and checks string types before
JSON serialization. No shared fixture is edited or regenerated.

`src/types.rs` adapts all 28 source interface declarations with 59 explicit
Rust records and inline-union enums. Source field spelling is retained, including
case-sensitive WebSocket pairs. Optional nonnullable fields use a present-value
decoder; optional nullable fields distinguish absence from explicit null;
required nullable fields still require the key. Records refuse positional arrays,
while protocol tuples remain arrays. Literal enums accept only source strings.

Every record captures `JsNumberInput` before buffering, applies the shared object
shape guard, and explicitly registers its complete `JsNumberOutput` graph using
an expression macro. Generic API response data must be an eligible typed value
or retained `JsNumberInput`; it must not be an unmarked `serde_json::Value`.
Numeric ingress is checked through raw, owned and borrowed paths before wire
serialization. The tests capture native fields and f64 bits independently, then
compare serialization after the shared `js_json_stringify` wire projection with
actual JavaScript `JSON.stringify`. Serde alone is not the JavaScript numeric
wire projector. These are declared type shapes, **not** runtime response admission
policies for future venue methods; preserve each method's actual acceptance and
fallback behavior when composing the adapter.

`tests/type_schema.cjs` reads the actual TypeScript declarations with a fail-closed
AST census, including heritage. The frozen local schema and explicit native
field probes detect drift and symmetric swaps. The oracle compiles typed positive
and negative witnesses against the actual declarations, then executes that module.
The 27 string-bearing rows cite the shared UTF-8 divergence; the numeric-only
`BackpackTime` row is ported. Lone-surrogate acceptance is captured as TypeScript
string type/code units before serialization; Rust refuses the same escaped input.

The complete REST adapter and WebSockets remain pending; configured account-method
bodies are implemented below. Signing request/header
composition and verification are implemented below.
Use the landed [exchange core](../safe-cex-exchange/CLAUDE.md) for subsequent
adapter slices. Preserve source behavior and change only Backpack manifests.

Scoped gates use `cargo test -p safe-cex-venue-backpack --jobs 1`, plus separate
host and wasm Clippy invocations with `--all-targets --jobs 1 -- -D warnings`.

`src/api.rs` mirrors five pure API helpers. Window arithmetic uses f64 directly;
URL checks retain source substring semantics and method case sensitivity.
Auth collection takes the shared prepared `JsJsonValue` graph, preserves native
numeric bits and JavaScript own-key order, and returns an owned deep snapshot.
This deliberately excludes executable getters, host objects, symbols and sparse
arrays; witnesses capture their source behavior rather than imply JSON coverage.
The source batch alias and nested spread aliases are compared against Rust
snapshot isolation after mutating the original nested input in place.
`tests/api_helpers_oracle.cjs` transpiles the actual entire API module and exports
private helpers only for observation. Unused dependency stubs trip on access
except TypeScript's `__esModule` metadata probe. One Node process returns the
whole corpus; native fields/order/bits are checked before wire serialization.

`src/signing.rs` ports the pure sorted-query and payload builders. It sorts keys
by UTF-16 units, leaves keys/instructions unencoded, percent-encodes values with
the encodeURIComponent safe-byte set, and preserves source batch validation
order. Prepared values are lowered only for their String(value) observation:
numbers use shared JS spellings, BigInts become decimal strings, array nullish
slots become empty join slots, and own noncallable toString still refuses.
The existing shared js_string performs coercion; no JSON numeric roundtrip feeds
signing bytes. serde_json is now a runtime dependency for that internal coercion
view, never an unmarked JS-boundary number input or output.
The actual source signing module is executed in tests/signing_oracle.cjs with
tripwires for unused crypto/clock functions. UTF-8 and prepared-data exclusions
are explicit; request/header composition has its own actual-crypto witness below.


`sign_request` composes the source payload builder, Node-compatible base64 caller
policy, and shared Ed25519 signing. `generate_auth_headers` preserves the caller
API key, source field order and number spelling, default window, and eager
clock capture before returning its future. The clock/pre-await snapshot must not
move into a lazy async body. Rust futures and name/message error strings are an
explicit host boundary; they do not claim JS job-queue or Error-object identity.

`tests/auth_oracle.cjs` executes the actual signing module and actual installed
Noble crypto, injecting only the controlled virtual clock. One Node invocation
returns 198 signer and 264 header cases, including permissive/malformed base64,
error precedence, UTF-8 boundary observations, and a call-time clock witness.
Production signature, header timestamp, and deferred-clock mutations each fail
named assertions and pass after restoration. No live REST transport is claimed.

Cardinality witnesses pin source totals and per-record/subcorpus counts before
loops, and separately assert completed comparisons. Keep full native-value
assertions after the explicit nested length checks; they independently detect
consuming decoder loss when count guards are disabled. The permanent
`tests/consuming_controls.py` mutates each of the seven concrete wire Vec fields
with a SeqAccess adapter that consumes every element before returning None.
Its `--without-cardinality-guards` companion must still fail the original named
native-value assertion. Both modes restore production/tests and rerun the scoped
target. Never run them alongside another Cargo invocation. See
[CARDINALITY_AUDIT.md](CARDINALITY_AUDIT.md) for the census, precise mutation scope,
105 control/restore pairs, and the distinction between corpus-loss false greens
and already-covered production collection loss.

`verify_signature` now composes payload construction, UTF-8 message bytes,
Node base64 decoding and shared verification, converting errors to false as the
source catch does. Its 507-case actual-TypeScript corpus asserts native booleans
and independently pins every subgroup and completed comparison total. The shared
strict Ed25519 verifier refuses weak identity keys which actual Noble ZIP215
accepts; three explicit cases witness that declared divergence. Do not replace
the shared primitive with venue-local crypto to hide the policy difference.

`order_utils` mirrors the three private status/time-in-force switch helpers.
`map_order_status` and `map_backpack_time_in_force` preserve case-sensitive
matching and source defaults; `map_time_in_force` accepts the shared typed enum
plus absence and preserves PostOnly's GTC result (postOnly is handled elsewhere).
The source witness loads the entire actual exchange module, actual shared enums
and actual venue constants. Only constructor transport/socket seams are inert;
unused dependency accesses trip. It does not exercise or claim the constructor,
REST adapter, subscriptions or store writes. Native enum variants are compared
before any possible wire encoding; fixed subgroup and completed counts protect
the corpus generator. Unicode positives and lone-surrogate ingress refusals
record the shared string boundary for the two string-input helpers.

`is_ordinary_limit_order` directly mirrors the source presence predicate using
shared PlaceOrderOpts. Presence of trigger/take-profit/stop-loss fields matters
even for zero, negative zero, NaN or infinities; close is rejected only when true.
The576-case actual-source witness covers all six order types,32 field-presence
masks and three close states, with native boolean assertions and fixed counts.
The shared typed ingress refuses five explicit-null fields and lone-surrogate
strings; actual source observations and an accepted Unicode positive document
those exclusions. A production truthiness mutation on trigger_price fails the
negative-zero case by name. No price/amount step formatter is implemented here.


`account_reads` composes eight account-method bodies with configured transport,
state, event and logging callbacks; it does not claim the venue constructor or
createAPI. `fetch_account_info` preserves parseFloat(value) || 1 and retains
prior leverage on caught read/coercion failures. `get_account` preserves the
truthy accountId || userId choice and explicit null affiliate ID; it logs then
rethrows, with a logger failure taking precedence. Native prepared values retain
BigInt, undefined, NaN, signed zero and nested fields without a JSON roundtrip.
`get_position_mode` returns the source Hedged UI enum. Unsupported position and
margin changes emit their exact error before warning logging; effects execute
at construction because the source methods contain no await, and callback errors
are returned as failed futures with shared error metadata preserved.
`tests/account_read_oracle.cjs` executes all five actual TypeScript methods.
Scoped native assertions cover 98 leverage cases, 52 account identity cases,
12 unsupported-mode cases, one position-mode result, and deferred/string
boundaries. Prepared-data, UTF-8 and host-error identity exclusions remain
explicit in the divergent manifest rows.

The type witness independently inventories all 17 top-level Boolean one-hot
supplements across ten records. It pins their unique identities and raw/source
native Boolean patterns, then checks decoded native patterns on all three
routes before wire assertions. The existing 1,545-case size pin alone cannot
catch homogenized one-hot payloads; keep the independent content expectations.


`order_utils` also implements four source helpers for subsequent batch/trigger
composition. `next_batch_client_id` uses configured compatible one-element uint32
crypto and f64 fallback providers, preferring crypto and never retrying a crypto
failure with the fallback. `create_batch_client_ids` preserves insertion order,
finite integer/nonzero uint32 validation, the source's 64-attempt ceiling and f64
count semantics, including fractional/NaN/negative counts and exact JS number
spelling in errors. The provider and generator callbacks retain shared error
metadata. This slice does not choose or implement an ambient native RNG policy.
`get_trigger_by` always performs the configured backpack metadata lookup, then
selects explicit type or kind-specific default and maps to the existing native
request trigger enum. The pending shared metadata table is not claimed here.
`determine_trigger_order_type` uses the first matching ticker and last ?? mark;
zero/NaN last prices do not fall through to mark, and instead return StopLoss.
Strict side-dependent comparison preserves source equality and infinity behavior.
A borrowed numeric ticker projection retains nullish fallback without requiring
the full typed Ticker record. UTF-8 symbol ingress retains its declared divergence.
The actual-source witness `tests/order_helpers_oracle.cjs` supplies deterministic
entropy/metadata/store seams and captures 16 RNG cases plus two failures, 14
unique-ID cases, 128 trigger-selection cases and 168 classification cases plus
two string-boundary observations. Native bits, enum variants, ordered IDs,
callback calls and errors are asserted with fixed independent corpus/attempt
counts. No native order placement or full venue/store wiring is claimed.


Four further runtime methods are implemented through configured dependencies.
`dispose` marks the instance disposed before public/private cleanup, stops on
failure, and passes generic opaque errors through without coercion or an error
ABI. `set_leverage` builds the complete leverageLimit PATCH graph eagerly,
commits leverage only after patch success, then logs/refetches and submits a
PositionsUpdate payload retaining the fetched value unchanged. Failed logging,
refetching or updates do not roll back a committed leverage. The shared error
and emitted-error representation is a named choice: executable arbitrary thrown
objects are witnessed in source only, with no invented native admission gate.
`cancel_symbol_orders` preserves the missing-market return and eagerly builds
the complete data.symbol DELETE configuration; fallible lookup/transport/log
callbacks preserve eager synchronous failures, awaited failures and logger
replacement. These callbacks do not impose or claim a full transport/store API.
`decode_batch_order_response` uses own-property prepared values and the typed
request record, preserving source validation order, String(operation) failures,
uint32 correlation, first-error precedence, ECMAScript trim and request output
order. Decoder and cancellation rows retain UTF-8 and host/prepared-domain
choices; executable getters/prototypes and arbitrary request/market properties
are outside the view. `tests/runtime_methods_oracle.cjs` executes all four actual
source methods. The native witness independently constructs/pins 47 decoder
inputs and requests, six disposal stages, 45 leverage value/stage pairs, and six
cancellation stages, plus opaque error/UTF-8 boundaries. It checks decoded/native
IDs, state bits, complete request payload fields, callback sequencing and error
metadata/identity before any wire comparisons. Full venue/socket wiring remains
pending; no live trading requests are claimed by these callback compositions.

`fetch_balance` reads the four collateral fields in source order, applies only
nullish zero defaults, and preserves shared String/parseFloat results including
NaN, infinities and signed zero. Its typed Balance has absent optional fields;
the shared declaration serializer's key order differs from this method's source
literal, so native field parity does not imply identical wire order.
`validate_account` distinguishes absent balance after capital failure from own
undefined after awaited balance rejection. A synchronous fetch-balance factory
failure still enters validation catch. Raw prepared truthy errors remain values,
not strings; selected errors are read again after emission. Scalar mutations of
the shared prepared error handle are observed, while executable/prototype and
object-alias semantics are explicit choices outside that view.
`cancel_orders` uses a live length/element provider and generic item/error types,
mirroring the installed p-iteration implementation's hole skipping and awaited
items/callbacks. Collection errors escape; individual cancellation errors are
swallowed. `cancel_all_orders` composes that algorithm with complete native
ORDERS DELETE data.symbol configs, per-market failure continuation and outer
logging/rethrow behavior. Generic cancel-orders admission adds no string/error
ABI; the all-markets wrapper retains its typed-ID/shared-error boundary.
The actual source and actual installed p-iteration execute in
`tests/validation_balance_oracle.cjs`. Native tests pin 48 balance cases plus
four transport failures, 110 validation cases plus mutation-after-emission,
22 serial cases and two explicit deferred element/callback scenarios. They
assert every native balance bit, raw error value, balance own-property state,
item/balance identity, request fields, sequencing and failure precedence.
Future scheduling and full venue/store/transport integration remain unclaimed.

`order_utils::map_order` implements the complete REST order mapping over the
existing BackpackOrder/Order declarations and configured classifier/logger.
It preserves trigger display/quantity selection, NaN/zero amount fallback,
warning rejection, independent filled/remaining arithmetic and reduce-only
defaults. The typed UTF-8 input excludes executable host properties, arbitrary
runtime values and null-present optional fields; the source accepts the latter
in a witnessed case. Native field equality does not imply identical serializer
field order. `tests/order_mapping.rs` independently pins 32 inputs times three
failure modes, checking native fields, bits, effects and opaque error identity.

`private_order_utils` maps the private WebSocket type/status/time-in-force values
from UTF-16 strings and classifies triggers using generic symbol equality.
Unknown strings, including lone surrogates, retain the source defaults; Unicode
Kelvin-sign casing is recognized. Trigger classification chooses the first
matching ticker, uses last ?? mark, defaults for falsy current price or absent
trigger, and retains source strict comparisons, including NaN. The oracle loads
the actual private socket source. Native tests pin 26 mapping inputs and 198
classifier cases, including duplicate tickers, signed zero, nonfinite prices,
absent triggers and a lone-surrogate symbol. Socket construction/wiring remains
pending; these are complete helper bodies over compatible native inputs.

`batch_orders::build_batch_limit_order_request` implements complete ordered
preflight and request construction over the existing typed options/request and
a prepared market view. It uses reciprocal Math.round step rounding and source
Number-to-String-derived decimals, including the source's scientific-notation
formatting behavior. Typed UTF-8/property/error views and request serializer
field order remain CHOSEN boundaries. Native tests pin 39 complete inputs,
13 requests and 26 errors, checking every emitted field before source shape.
`batch_orders::place_orders` validates caller IDs first, captures both candidates
before eligibility and uses live collection reads after each completed call.
Pair failures report; single failures coerce and log their exact source message
in UTF-16. Generic IDs/errors and paired order values impose no new string/error
ABI. Tests pin 19 modes, native ID contents, paired-item/error identities and
independent eager/final counts, including holes, live mutations, sync/async
failures and a deferred pair. Generic ID pointer identity, hostile collection
or iterator prototypes, full JavaScript scheduling and venue wiring are not
claimed by these compatible native views.

`can_use_native_batch` implements the full short-circuit eligibility body over
compatible option/predicate/client-ID/builder callbacks. Predicate and property
access failures escape; only builder failures become false. `place_batch_pair`
generates IDs and builds both rows before its single eager POST, awaits the
response and gives decoding the same live Rc/RefCell request array. Generic
row/data/result/error types preserve identity and admit UTF-16 results without
a new string/error ABI. Parent helper rows and full exchange wiring are tracked
separately. Compatible declared readonly pair inputs are used; hostile array
prototypes/access and the full JavaScript scheduler are not claimed.
`cancel_order` looks up the market and builds the complete ORDER DELETE
data.symbol/data.orderId config. Missing market and request errors log and
rethrow, with logger replacement. Its typed UTF-8 IDs and shared error view are
explicit choices consistent with the other cancellation wrappers.
`tests/order_actions.rs` independently pins nine eligibility records, nine pair
stages, ten cancellation records and a deferred-response case. It checks native
booleans, payload fields, preflight order, callback counts, original errors,
live request/data/result identities and UTF-16 result contents before source
trace comparisons. No live order placement or cancellation is performed.

`tests/exchange_trait.rs` constructs `BackpackExchange` and drives all 16
`not_implemented()` defaults of the `Exchange` trait through `dyn Exchange` on a
scripted transport. TS backpack overrides neither `fetchSymbolMaxLeverage`,
`scheduleCancel` nor `fetchPublicTrades`, so their Rust overrides return the
inherited `base.ts` 'Not implemented' rejection; do not invent venue bodies.

The SDK market projection retains `limits.price.min` from the source price
filters, so the portable `assertMinimumPrice` guard can refuse prices below the
venue minimum. Only `isSpot` is present in the source SDK flags; CCXT's separate
`swap`, `future` and `inverse` fields do not create SDK `isPerpetual`, `isExpiring`
or `isInverse` fields. `tests/market_projection.rs` drives the real HTTP binding
and executes the actual TypeScript price guard on its serialized market output.

The adapter's `native_value::Value` is an alias of the shared `js_runtime::Js`,
so HTTP/socket frames and runtime callers share the same object/array references.
`NativeValue` retains the existing prepared-data/checked UTF-8 projection profile;
it does not own a second value enum. Shared native numbers and UTF-16 strings stay
intact until that explicit projection. The constructed throttle regression in
`tests/adapter_classes.rs` checks deferred reads of runtime-mutated aliases, native
numeric bits, nested spread identity, holes and lone-surrogate projection refusal.

Concrete disposal adapters follow their source overrides rather than invoking
superclass disposal. The exchange sets the shared disposed flag before public
then private cleanup. Public disposal clears subscriptions, book state/fetchers
and depth throttles; private disposal clears subscriptions and the orders timer.
Other handlers, ticker throttles, position/balance state, Store subscriptions and
source-uncancelled work remain live. `BaseWebSocket::close_current_websocket`
forwards only the current transport close. Three native regressions prove-fail
against the old adapters and pass against the actual source disposal arrows;
`tests/disposal_binding.rs` also checks flag timing and close ordering.

Native `native_inputs_signed` shares the account’s hosted HTTP signing capability
through the common HttpClient bridge and retains public-request bypass. The
registry also exposes that capability to typed canSign. Private WebSocket auth
uses the local Ed25519 secret exactly as the source: a missing secret logs the
original RangeError and sends no subscription, even when HTTP is hosted. See
[the hosted signer evidence](HOSTED_SIGNER_EVIDENCE.md); no hosted WS path is added.

The API factory carries the complete shared ExchangeOptions through native
construction and adapter installation, including proxy flags, region, overrides
and public-only state. Requests inherit the shared HTTP factory's routed base
URL and 20-second timeout. The native_routing regressions run native_inputs,
the production Exchange binding and actual Direct/Tealstreet providers to check
direct-transport refusal, override routing, JA routing/authorization and timeout.

Private reconnects use the inherited orders-only snapshot refresh. They must not
call `tick`, which schedules another recurring account/position/balance poll.
`tests/private_reopen.rs` drives the real socket lifecycle and authentication,
checks that reconnect refreshes orders, and advances the runtime clock to verify
that the original polling chain remains the only one.
The same socket-event fixture covers private order and position Store commits,
fill emission, and the real REST balance refresh triggered by an order-fill frame.

The constructed native HTTP client acquires its rate slot before invoking the signing interceptor, including network retry attempts. `tests/rate_limit_signing.rs` fills the real ten-request window and verifies the queued request reads its timestamp after dequeue.


The assembled HTTP client now selects its separately constructed secondary client
for GET reads. Each key keeps its own signing interceptors and resolved route;
write methods retain the primary client and its ten-request budget. Secondary
reads have no rate wrapper. The HTTP builder is reusable so each client receives
fresh shared factory state. Secondary options preserve absent versus empty
credentials and replace the primary proxy override with the source's secondary
proxy or provider public CORS default. Native routing regressions exercise the
real binding/factory, verify the secondary signature against an RFC8032 public
key, and distinguish the separate rate budgets.

Native REST and private subscription signatures read the synchronized shared
virtual clock. REST ticker IDs and position timestamps use that same source
clock. Rate limiting, heartbeat timing and private refresh throttles retain their
source monotonic or Date.now clocks. The native routing and private reopen
regressions distinguish synced time from an unrelated host clock.

The constructed ticker corpus in `tests/exchange_trait.rs` observes 72 cases / 216 ticker, trade, bookTicker and depth frames
through the production socket protocol and DefaultStore. Independent null,
string, NaN-producing, object, overflow, signed-zero and false fields accompany
distinct good rows. The maintained TS constructor and actual DefaultStore provide
native numeric-bit expectations. Fresh TS receivers and advancing native clocks
isolate numeric mapping from throttle timing. `ticker-native-controls.py backpack`
compiles a live publication fault, requires a native assertion failure, then
restores exact source bytes and requires the named test to pass.

The concrete leverage catch emits the helper-created native Error value rather
than its message string. The constructed Exchange regression drives a successful
account PATCH followed by a malformed position refresh, checks the committed
leverage and single refresh, and inspects Error name/message before projection.

Native HTTP executions belong to BaseExchange shared operations. A rejected
ticker Promise.all drops its waiter while the already-started mark-price request
continues through response interceptors. The native registry proxy test holds
that response until after fetchTickers rejects, then observes both error events.

The adapter HTTP fixtures drain the actual exchange-owned runtime jobs before
asserting completion. The checkpoint driver never advances the clock: rate-bound
requests remain pending until the test explicitly opens the next window. The
constructed-client proof observes pending work before pumping, one real terminal
request afterward, unchanged clock bits, and the original response. Primary and
secondary budgets and signing-after-dequeue assertions remain in their existing
real-client tests. A single initial Future poll is not a completion guarantee.

The API/signing corpus captures lone-surrogate prepared values through canonical
retained numeric ingress and compares native UTF-16 units. Legacy UTF-8 prepared
helper/output limits are separate from that capture; their existing source cases remain.

Successful native HTTP responses retain canonical `RetainedJsonInput` through the
response interceptor and native callback. Encoded bodies use the shared Axios
transform once; already transformed data is forwarded directly. Unread UTF-16
keys/values, own undefined and nonfinite numeric metadata reach model `info`
without a JSON projection. Request/signing prepared inputs retain their existing
typed policy. The two native routing regressions fail on the old response bridge
and compare the maintained source interceptor/market method before native checks.

The accepted receiver inventory reviews `MessageValue::equals_string` and
`NativeValue::field` on the shared Js receiver with exact compiler impl hashes.
The two native receiver tests in `adapter_classes.rs` cover strict UTF-16
comparison and own-field forwarding, including native cell identity, presence,
number bits and nullish refusal. These entries classify existing receiver roles;
they do not widen the adapter's prepared-data profile.

The Backpack alias census finds 16 live structural containers: nine have native
Serde decoders, and seven hold already typed state, callbacks or prepared values.
The seven generic containers register one fixed instantiation with exact compiler
source and expanded targets. `scripts/container-alias-contracts.mjs` authenticates
the full compiler inventory before running this finite cohort; the separate 18
callable/future and two nominal aliases are outside this cohort.

Native decoder observations inspect UTF-16 keys, duplicate-key order, holes,
number bits, shared ownership and mutation visibility without JSON projection.
The four container tests in `adapter_classes.rs` also construct the actual typed
aliases, including `SigningRecord<Js>`, preserving native nonfinite values, own
undefined and original cell identity. These observations cover the declared
instantiations and prepared profiles, not every possible generic value type.

The remaining alias cohort has 13 callable capabilities, five pinned Result
futures and two nominal option aliases. The existing Rc/Fn family authenticates
its generic callbacks; exact fixed registrations handle the Result futures and
one-shot boxed callbacks. These native capabilities are non-decoders. The
concrete ExchangeOptions alias instead retains its shared strict decoder, while
the prepared order-option specialization has no raw decoder.

`callable-alias-contracts.mjs` authenticates the full compiler inventory before
selecting all 20 owners. Native tests in `adapter_classes.rs` invoke the actual
aliases, preserve arguments/results and shared effects, poll each pending future,
and execute production native HTTP factory/install and Date.parse callbacks.
The finite instantiations do not claim every generic argument or JavaScript
scheduler behavior.


`RoutedClient::clone` shares the configured primary and reader client owners. The
construction policy records only Clone; it adds no decoder or receiver exemption.
The native adapter witness checks both Rc counts, shared retry state, primary
identity and real GET/write routing after the original handle is dropped. A
compiled swapped-owner Clone control must fail that witness before exact restore.

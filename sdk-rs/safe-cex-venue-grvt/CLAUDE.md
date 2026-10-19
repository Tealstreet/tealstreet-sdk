# GRVT venue port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/grvt/CLAUDE.md).

The first slice implements five protocol tables: all 44 endpoint/URL/telemetry
entries are compared in order with the actual TypeScript module at test time.
Eleven protocol string enums now require string-only deserialization and compare
44 literals against the live TypeScript checker, plus 748 refusal probes.
Their eleven private Wire decoders independently admit strings; public filtering
is not a private decoder guarantee. Native tests match all 44 variants before
wire checks and reject each tagged object through owned, borrowed, text and
qualified deserialization. Restoring the private derived decoders makes all
eleven named shape tests fail; retain that compiling regression control.
Twenty-three top-level protocol records use the shared map-only decoder and private metadata
companions. Optional typed fields reject explicit null; opaque payloads retain it. The 1,782-case TypeScript-checker
corpus includes 264 deliberate lone-surrogate cases. Canonical retained ingress
admits typed String replacements while direct Serde capture is measured
separately; raw opaque carriers retain their exact units.
All venue manifest rows now have mappings; each fixture establishes only its stated scope. Variants stay within this crate; consume
the [shared exchange trait](../safe-cex-exchange/CLAUDE.md) and route missing
runtime or signing primitives to their owner.

The mandatory root gate is `yarn venue-oracle:grvt`. It requires an absolute
isolated `SAFE_CEX_TYPESCRIPT_PATH`; missing compiler installation fails closed.
See [LOOKUP_CENSUS.md](LOOKUP_CENSUS.md) for the initial 95-expression inventory,
including ten direct wire-keyed reads needing source-path witnesses.

GrvtInstrumentLite captures `JsNumberInput` inside its public Deserialize body
before the private map-only codec. This is depth 2 adoption: bare raw, owned and
borrowed Serde calls are covered, as are the shared entry helpers. Its bd/qd/fi
fields are binary64 Number values, including signed zero, overflow and negative
underflow; compare bits before serialization. The prior clean map-only candidate
was never committed and is a fixture, not a shipped regression guard. The REST
fetchMarkets implementation exists; type-entry evidence does not establish its adapter binding.

GrvtSignature, OrderPayload, GrvtBuilderCheckResult and GrvtBuilderInfo use the
same depth-2 capture. Signature v/nonce and builder raw fee/cap fields remain f64:
TypeScript number does not imply an exact uint32/u64 acceptance contract. Type
ports do not implement the signing or authorization runtime. Keep OrderLeg's
recursive eligibility exhaustive when embedded by OrderPayload.

WebSocket wire records use closed JSON-RPC/version and subscription-method
unions and map-only nested params/error objects. Opaque params/result/error.data
retain JsNumberInput, including explicit null and nonfinite Number values before
wire serialization; they are a JSON data domain, not executable JS any. Optional
opaque fields use present capture so null is not silently treated as absence.
Socket runtime and dispatch are mapped; numeric tests exercise public
Deserialize through bare and shared entry paths, not a socket lifecycle.

Builder constants execute from actual grvt.builder.ts declarations in the live
oracle: both network addresses, the mainnet legacy alias and default fee string.
The unauthorized-reason union has a string-only decoder. These declarations do
not claim browser authorization or wallet signing is ported.

GrvtTriggerMetadata uses depth-2 public numeric capture and the shared
OrderStopTriggerType, with a local closed GrvtTriggerKind for the inline union.
The actual TypeScript enum supplies member expressions in checker probes; no
string-enum acceptance is inferred from a handwritten list. triggerPrice retains
Number bits on six public/shared ingress routes; order_semantics now implements
the deriving helper against actual-source runtime cases.

Builder address selection and fee conversion execute against their actual source
functions in builder-helpers.cjs. Fee conversion delegates parseFloat and
Math.round to shared num; it returns f64, not uint32. Preserve overflow created
by multiplication after the source's finite-input check, and preserve signed zero.
The Result error carries the source Error message; invalid string representation
uses the canonical UTF-8 boundary rather than replacement characters. These pure
helpers have no outgoing Store, reconnect, health or notification path.

create_grvt_builder_info consumes those helpers and returns the existing builder
record. None selects each source default; empty address/fee strings remain
present. Preserve the original fee text independently of its converted Number.
Actual-source tests cover every Unicode scalar in lowercase contexts; the UTF-8
boundary remains declared. Their runtime bodies are mapped; these pure-helper tests do not establish their adapters.

GrvtBuilderUnauthorizedError carries source name/code/reason and f64 cap values;
its constructor uses shared Number stringification for the cap-too-low message.
Absent authorized cap means unknown, while zero remains zero. An empty override
is retained and Error display omits the colon in that case. The native error has
no JS prototype or JS stack; both rows declare this separate chosen boundary.

Order-semantics helpers accept plain JSON records, preserve nullish full/lite
alias precedence and delegate Number conversion to shared num. Trigger price
retains finite signed zero; nonfinite results are omitted as in source. Runtime
host objects remain outside this chosen input representation. Order-mapping
callers are mapped; neither pure-helper fixture establishes their Store effects.

AST numeric and interface-derived union inventories refuse heritage clauses.
Four permanent controls inject an inherited required number and require a named
refusal, preventing a direct .members walk from silently measuring a subset.

Record and union gates assert native field/variant identity before wire checks.
Keep the explicit native-to-wire test mapping independent of Serde annotations;
a roundtrip alone cannot detect a symmetric rename. Field-wise fixtures vary
siblings independently; the audit and controls live in tests/FIELD_IDENTITY_AUDIT.md.
Builder-error oracle captures canonicalize NaN only and label host architecture;
finite values and signed zero remain bit-exact.

Signing metadata validation reads ih/bd/qd directly from retained info. Hashes require
lowercase 0x, 1..64 hex digits; exponents must be Number safe integers, including
negative values and signed zero. Actual-method oracle checks native tuple fields
and Error messages in one Node process per corpus. The mapped
placeOrder signer and fetchMarkets have separate tests; this validator fixture
carries no Store notification effects.
The validated ASCII hash becomes an owned String at the signer API; its original
UTF-16 units remain in info. Exponent bits and the explicit non-borrowed hash
observation precede serialization. The native trigger alias selector stays borrowed
and preserves own undefined/null precedence without constructing a JSON view.
The source nonnegative-exponent mutation and a compiling Rust bd/qd swap fail
named SIGNING METADATA assertions; production is restored afterwards. The
validator returns native fields, so serialization cannot undo a swapped result.

Instrument-symbol conversion preserves the prefix and normalizes only a terminal
ASCII case-insensitive _PERP. One actual-method corpus covers 1,128 strings and
two double-escaped lone-surrogate refusals; no locale case folding or trimming.
An unanchored source mutation and a compiling case-sensitive Rust mutation fail
`INSTRUMENT SYMBOL suffix identity`; restoring production passes the same gate.

Signing time-in-force conversion uses the closed GrvtTimeInForce enum and returns
Number values 1/2/3/4. The sole placeOrder caller chooses three literal values;
ALL_OR_NONE completes the declared union. Prototype keys and unknown fallbacks
are unreachable from that caller; no arbitrary-string input contract is claimed.
The executable source is authoritative over its stale zero-based comment: the
zero-based source control and a compiling 2/3 Rust swap fail named native variant
assertions. Both mutations are restored before reporting the slice.

Legacy exported signing-clock helpers take one injected Date.now read per call.
Nonce retains Number bits; expiration retains source operation order and Number
stringification, without integer narrowing or nanosecond truncation. These unused
exports are measured directly and do not claim the active GrvtSigner path.
One Node process returns all 462 clock cases. Compiling nonce-sign and expiration
truncation controls fail SIGNING CLOCK nonce bits and expiration text assertions;
clock-read counts are checked before any serialization can hide their effects.

Interval conversion preserves the twelve own mappings and delegates stock inherited
lookups to TableHit; only missing entries take the one-minute fallback. The sole
fetchOHLCV caller has closed Timeframe inputs covering exactly the twelve keys;
inherited names and fallback inputs exercise the broader helper contract only.
The actual-source oracle compares native variants and inherited identity before
serialization, with three input-only UTF-8 refusals. The mapped OHLCV runtime has separate tests; this interval fixture does not prove its adapters.

The collection-cardinality supplement repeats the admitted source element to
exercise partial retention for all five Vec fields. It retains the original
checker corpus unchanged; native lengths are checked before pairwise identity.

Opaque WebSocket cardinality checks use two-element/member string collections
for each payload field, independently of numeric projection tests. Compare full
opaque values and count returned members; see tests/CARDINALITY_AUDIT.md.

Builder EIP-712 domain construction preserves name GRVT Exchange, version 0
and chain IDs 325/326. Do not substitute the legacy signOrder domain. The live
source helper is measured independently; browser authorization has separate runtime evidence.

Builder address selection returns the first shared-crypto-valid original string,
without normalization. get_string_field does not coerce nonstrings or empty text.
The actual-source oracle consumes SAFE_CEX_CRYPTO_ORACLE_DEPS (ethers 6.17.0),
checks ordered candidates and native identity; browser/authentication bindings have separate evidence.
Compiling last-candidate and empty-string-drop controls fail the native address
and string identity assertions, respectively. Both mutations are restored.

Supplementary refusal/boundary arrays have independent source and completed-check
counts, using the shared num test-only walker. Their controls mutate captured
expectations, not production collection outputs or original source artifacts.

Builder proxy helpers consume the shared TransportRegistry on every invocation.
Header resolution first-polls its pinned frame before return and retains the
selected proxy across a pending result and registry replacement. Native prefix,
counting-waker and header-value witnesses cover this path. The remaining shared
PrivateProxy BTreeMap/borrowed-future limits are FORCED in the row, including
sorted headers and discarded-future cancellation versus a retained JS Promise.
Canonical classification: [MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

fetchMarkets is no longer blocked on error-string visibility. Its source witness
executes the complete common and GRVT Axios factories and extracted method body,
with real Axios/retry and injected raw adapter, transport and telemetry observers.
Eleven fixed raw cases pin response phase, distinct steps, signed-zero/overflow
info and logging-failure order. Independent raw-input content and order checks
reject same-length symbol replacement and reversal. This is source-path evidence,
not constructor/rate-limit proof or a remote
signing/browser claim. The mapped native fetchMarkets runtime has separate tests.

Snapshot methods now consume the shared Store and caller-owned nonce maps.
Preserve earliest nonce time and paired 200-entry eviction, native Number P&L
order, position notifications before the live balance read, and original
subscriber errors. Ticker snapshots and delegation hooks execute eager effects;
account identity is read after its request awaits. Their configured client and
socket hooks do not claim the constructor or unimplemented runtime dependencies.
Fake-position inherited leverage is a declared closed typed output refusal.
See tests/snapshots.cjs and snapshots.rs for the actual-source bounded corpus.

Order snapshot helpers map raw JSON statuses to native OrderStatus and parse
metadata creation times without narrowing nanosecond integers. Keep nullish
create_time/ct precedence and divide the digit string before converting the
quotient to Number; numeric inputs first use shared Number stringification.
Actual-source native assertions cover 47 statuses and 1,450 timestamp inputs.
These extracted helpers have no Store effects; mapOrder has separate runtime tests. JSON-data and shared UTF-8 input boundaries are declared in their rows.

Market reads now use src/markets.rs and the shared optional Market.is_perpetual.
The configured post hook supplies already transformed response.data. Keep native
representation admission after source mapping and logging; earlier malformed base
fields must not conceal a later null row's source error. Strict base/quote refusals
now omit only that row and log its index/id and exact field values at Warning,
under the approved per-row policy; valid surrounding markets remain available. Refresh always replaces
signing metadata before membership events. Its old snapshots and callback values
are owned; the manifest names the measured difference from live JS aliases.

Order controls now use src/order_controls.rs with bound SDK and socket hooks.
Keep eager guard/log/first-call effects, sequential batch continuation, authentic
builder-error short circuit, and replacement validation before cancellation.
Missing/null trigger info keeps the source property-access error; the constructed member captures raw enumerable update-key count before typed
projection, preserving fieldCount even for extra own keys. The pure typed helper
counts only represented fields because it has no raw caller object.
These methods do not claim the SDK constructor or single-order placement path.

GRVT public candles/books and account leverage are in `data_reads.rs`. Configured client hooks supply transformed HTTP data or SDK responses; KlineRequest keeps native f64 limits and optional IDs until serialization. Keep current-first order-book total accumulation and cache notification before the fresh position lookup. Leverage helpers declare owned Market/returned-cache alias boundaries in the manifest and assert corresponding source/native witnesses; the account getter is live after setter logging.

Public websocket subscription methods are in `public_subscriptions.rs`; the full websocket constructor/connection has separate runtime tests. Keep native RPC fields visible to configured send hooks before their shared-JS JSON wire projection, and preserve SameValueZero subscription keys versus strict ticker-ID cleanup. Batch timer slots remain tombstoned while cancellation iterators are live so nested cancellation plus insertion does not skip new timers; compact only after the outermost iterator ends.

Public websocket lifecycle is in `public_lifecycle.rs`: configured factory/listener/emitter/timer hooks preserve socket and callback rereads. Market event arrays and batch slices reference shared Market values through Rc/RefCell; drop all borrows before invoking hooks, reread live array length during scheduling and keep each captured row reference during callbacks. SubscriptionMap keeps tombstoned insertion slots while replay iterators are active so nested replay/clear/reinsert remains live; unused slots compact at the outermost iterator exit. Constructor and feed/listener handling have separate runtime tests.

GRVT private websocket command ports live in `private_commands.rs`: authentication, guarded ping/open, retry-budget reset, fail-closed authentication, shared-feed subscription dispatch and disposal. Keep the feed `Rc` shared between sent and stored subscriptions; reread the healthy marker after clock callbacks, preserve default logger severity and the own undefined account slot before wire projection. Configured clock/logger/send/dispatch/timer/base-dispose hooks preserve observable ordering and errors; the private constructor, watchdog/refresh and update handlers have separate runtime tests. `private_commands` tests execute the actual TypeScript members and logging enum, assert native fields/state/identity before deferred wire assertions, and stop at 67 fixed cases.

Public feed routing and listeners live in `public_feeds.rs`. Bind Value/property/coercion and live Store operations at source call sites; ticker symbol and selector slots can contain own undefined values from inherited lookups. Keep selected handler arrays, feed identity, shared Market/Position references and source read order across callbacks. HandlerSlot distinguishes holes from explicit undefined during spread/filter/forEach. Order-book totals start from the current amount before rereading prior levels; sparse output finishes source processing and visibly fails because shared OrderBook vectors cannot retain holes (FORCED output component of the MIXED listener). The actual-source/native floor stops at 78 cases; public class/constructor have separate runtime tests.

The order-book listener resolves each live map member and invokes it with its
original receiver and a retainable native projector. The projector retains
opaque index/array arguments, repeats relational coercion and prior-element
reads, and creates ordered own data fields with unchanged Number bits. Both
map calls finish before pure dense-data admission to the shared OrderBook;
sparse, nonarray or accessor output visibly refuses the whole callback without
executing output getters. The ten-case supplement is public-map-source.cjs
and custom_orderbook_map_native; it does not establish a concrete JS host.

The 51-row divergent audit is DIVERGENCE_AUDIT.md. Its parsed-field unit
witnesses do not establish executable structural-object parity or all caller
bindings. Numeric-capture depth refusal is a separately priced local choice.

The production binding is `binding::exchange::GrvtExchange`, assembled from the
existing constructor, account/order runtimes and native public/private socket
ports through the shared `js_runtime`. The Rust registry constructs it without a
module-host fallback. Its SDK adapter uses the consumed lite schema maps, SDK
OrderWithBuilderFee signing domain, cookie refresh and endpoint paths; legacy
signing helpers keep their separate source domain. Live vendored-source oracles
compare signatures, bidirectional schema mapping and cookie parsing. The 23
production Exchange tests cover startup, all required methods, SDK wire requests,
nonce reconciliation and public/private socket behavior; 17 compiling dispatch
controls fail their named test before exact restoration.

This binding retains the existing typed JSON/Store and native cache boundaries
listed in the manifest. Socket scalar fields alias native state; map views read
that state and refuse arbitrary protocol-cache writes. It does not establish
arbitrary executable host-object or third-party SDK conformance. The five missing
source Exchange methods retain their inherited `Not implemented` rejection.
The native registry installs the shared hosted signer on GRVT's HTTP clients,
including public requests that the source does not tag for bypass. Hosted
`getAccount` works without a local secret; the provided callback owns returned
authentication config. SDK order placement retains the source local-wallet
requirement and fails before signing RPC or HTTP when that wallet is absent.
`safe-cex-registry/tests/native_host_grvt_signer.rs` compares actual source
factories and `getAccount` inputs, observes callback headers on real reqwest IO,
and checks callback rejection and the independent SDK wallet guard.

Each of the three API clients uses the shared
`js_runtime::client::factory::create` with the runtime transport provider. This retains eager routing
refusals, transport authorization, standard headers and the common 20-second
timeout. The source GRVT HOW installations remain commented out; this binding
preserves that choice. A caller-supplied HOW terminal still receives cookie/account
config extensions. `safe-cex-registry/tests/native_host_grvt.rs` exercises real
local HTTP, one-login SDK validation and direct header semantics; dropping the
proxy routing options compiles and fails the construction-time routing-refusal test.

The first accepted-input contract batch reviews 52 owners individually in the
per-crate contract file. Retain the 25 complete positional-array controls and
five native alias tests; a passing subset does not clear unreviewed compiler
owners or the borrowed-alias probe refusal. Exact scope and reproduction commands
are in [ACCEPTED_CONTRACTS.md](ACCEPTED_CONTRACTS.md).

Private numeric comparison preserves BigInt equality across both Number zero
signs. Native trait-method tests pin coercion effects, original thrown-object
identity and the source error-code comparisons to 1000. Current TypeScript call
sites use 1000; zero cases exercise the broader native comparison helper.

The assembly exposes its ten private scalar field accessors as ordinary Rust
closures and imports the client-ID rule type explicitly. The original four
numeric and six boolean macro calls produced local-macro discovery refusals;
the wildcard import also prevented macro-origin certainty. The source scanner
accepts the explicit assembly without an exception or shared guard change.

The private_scalar_views_keep_each_native_cell_live_and_independent unit test
inspects every native cell and its descriptor in both directions. Numeric writes
preserve Number bits and reject non-Numbers without changing state; boolean
writes retain the existing truthiness behavior. Wiring only requestId's setter
to the session-attempt counter compiles and fails the named native-write
assertion; exact restoration passes. This source visibility repair does not
classify the complete Host owner or any of its remaining raw APIs.

The exchange and member-dispatch bindings also name their consumed shared types
explicitly. Their former wildcard imports prevented macro-origin discovery. The
source scanner now retains the existing reviewed local comparison and reports no
further markers. The four inherited-refusal policies retain their exact compiler
APIs, method bodies, TypeScript sources and native witnesses; only the import-driven
Rust line spans and whole-file hash change. This visibility repair does not
classify the remaining unreviewed owners.

GRVT SDK OrderWithBuilderFee signing converts the builder field with MetaMask's
address rules in the local typed-data copy before shared EIP-712. Short hex and
either prefix casing are padded to 20 bytes; safe integer Numbers become bytes.
Other strings use the SDK dependency's legacy base-10 loop over UTF-16 units,
including its first-20-byte selection for oversized results. Preserve source
error names/messages, the caller's original value/reference, and integer-field
conversion after the address's Number conversion. The original 24-case signature
matrix remains; a separate constructed SDK host compares 24 independently
authored address inputs with the actual vendored Signer. Shared ethers address
validation and legacy signing retain their own boundaries.

The native `Exchange::refresh_markets` entry calls the existing GRVT receiver
override, which always replaces stored markets. It must not inherit the shared
membership-only refresh: unchanged IDs can carry new tick, limit and ih/bd/qd
signing metadata. Constructed trait regressions cover that replacement and the
added/removed native result projection.


The SDK signature oracle runs in the ordinary native lib test suite (no ignore
annotation). Its offline vendored-SDK matrix compares 24 complete signatures and
r/s/v values across both networks, order kinds, Number integer boundaries and
builder-address casing. It requires Node, TypeScript and vendored SDK dependencies;
missing dependencies fail the oracle rather than silently skipping signing checks.

The accepted signing-metadata row includes 66/67-character hash boundaries.
`exchange_binding` sends both through actual market loading and placement, comparing
with the maintained TS method: 66 characters signs/submits, 67 refuses before
builder lookup. `hash-length-binding-control.py` removes the production bound,
requires a compiled placement diagnostic failure and restores/reruns the witness.

The accepted builder fee contract includes negative-half and signed-zero bits.
The production placement witness feeds negative authorization caps through the
real client/builder check and verifies its refusal message before any order write.
`tests/negative-half-binding-control.py` swaps only Math.round for Rust round,
requires that binding assertion to fail after compilation, restores exact bytes
and requires the witness to pass. Native private frame tests also assert orders,
fill events, positions and the balance UPNL updated by a position event.

`convertInterval` preserves the twelve stock inherited table members in the
assembled class as native realm references. The typed OHLCV caller still uses the
closed Timeframe union; the receiver-level test calls the real constructor's
method, compares all 93 maintained-source cases, and checks inherited kind,
stringification, arity and repeated reference identity. The accepted helper row
now witnesses every inherited member (17 -> 27 cases).

`private_receive_watchdog_obeys_grvt_idle_thresholds` drives the real private socket
through authentication and the shared receive timer. GRVT stays silent at 8 seconds,
pings after 30 seconds and reconnects after 90 seconds with source Warning severity.
The shared socket bridge preserves lastReceived reads and dispatches the actual
GRVT override; the former base-only timer fails the 8-second assertion.

The uncalled generic api_runtime::last_segment copy and its isolated session
routing cases were removed. TypeScript lastSegment is an unexported helper of
isGrvtRead; production request classification retains the shared transport
predicate. The session oracle keeps its other eight groups (84 source cases).
The helper manifest row is not-applicable, with the live predicate mapped next
to it; this does not remove any exported builder or browser workflow.

The concrete amendment and cancellation catch callbacks invoke the installed
`emitAndLogError` reporter. Constructed binding regressions cover preflight
amendment refusal and SDK account/symbol cancellation failures: reporter metadata,
console error/trace effects and the absence of premature amendment writes must
survive. Direct emitter calls lose the source reporter's second argument.

Constructed candle coverage in `tests/exchange_binding.rs` drives REST and raw
public socket frames for lite/full aliases, nanosecond-string versus numeric
millisecond timestamps, truthy fallback, reverse REST order and callback cleanup.
It compares all six native fields against unchanged maintained-source captures,
including NaN, infinities and signed zero. Run `tests/candle-binding-controls.mjs`
alone for two compiling live-volume substitutions and exact restoration proof.

The constructed ticker corpus in `tests/exchange_binding.rs` covers 1,000 cases /
3,000 frames across lite/full snapshots, deltas, sparse mark fallbacks and
subscribed candle-close updates. Every numeric alias
receives ten independent values, including nonfinite results and distinct later
fallbacks. Volume aggregation, percentage derivation and string nanoseconds versus
numeric funding-time conversion use the real socket and Store. All eleven native
numbers match maintained TS bits; the shared fault runner swaps mark for index
and requires a compiled assertion failure followed by exact restoration.

The concrete public feed host passes an object with the market id to Store's
updateTicker, matching TS. A raw string selector silently misses every ticker;
the real startup/socket regression proves that master failure. The source candle
subscription uses the actual source WebSocket factory connected to a loopback
peer, while native frames enter the actual runtime sockets. Candle callbacks,
and zero/existing-last mark fallback have independent fixed assertions.

Market reads retain the native response before mapping. Signing info always owns
`ih`, `bd` and `qd`, including missing values as Undefined. The constructed REST /
Store regression inspects UTF-16 units, nonfinite numbers and signed-zero bits.

The shared retained typed decoder admits isolated UTF-16 escapes and replaces
them with U+FFFD only in typed String views. `tests/divergence_records.rs` keeps
the unchanged 22-record/7-captured source matrix and its 1,782-case input source,
checks the exact raw units independently, and observes canonical typed decoding
for every selected record. Bare Serde is a distinct route: seven numeric roots
capture retained text themselves, while the other selected records still reject
those strings when called directly with serde_json. This distinction does not
establish that every production socket/REST consumer uses canonical ingress.
The 264 boundary cases inspect every native field recursively against the
retained source tree before wire projection, including exact opaque string/key
units, collection lengths/order and numeric bits. The original direct Serde
route and ordinary Value-based native identity checks remain independently
required; an expected-value parser failure cannot stand in for decoder refusal.

The live SDK cookie parser follows locked set-cookie-parser date admission:
invalid Expires attributes are ignored, preserving any earlier valid date.
Without a valid expiry the SDK caches null and omits Cookie on its private
request. The original 18-case maintained helper corpus plus valid-then-invalid
and actual fetchPositions HTTP/cookie-reuse witnesses pin both branches.

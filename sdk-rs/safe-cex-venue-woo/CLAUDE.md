# WOO venue port

The canonical spread, lifecycle for-of and array-pattern slots consume the shared
iterator protocol. Spread retains native values and UTF-16 code points; loops
observe collection mutation during the body. Array patterns advance only their
selected length, skip value getters for elisions and close an unfinished iterator.
Body throws win over close failures, while step/value failures do not close.
`woo_host_*` tests use the constructed exchange’s actual operation tables; four
regressions compiled and failed on master. The source private socket subscribes
only to executionreport/algoexecutionreportv2: fills trigger REST position refresh,
and balance remains a REST read. `binding_streams` and `binding_reads` retain those
real signed-client paths; no balance socket topic is introduced.

The V3 canonical/shadow/snapshot comparison bodies were test-only: the SDK index
exports none of them and their TS imports occur only in audit tests. Their Rust
bodies, unused operation holders and oracle-only tests are retired; tealstreet-next
owns the test-only TS cleanup. CanonicalOps/CanonicalCallback remain the exact
live legacy operation shape. The independent route enum/strict union codec stays
with its declaration witness. Legacy client-ID validation, including the
constructed class's Number.isSafeInteger call, is retained for separate binding
verification. Four dead-method baseline entries and five capability exemptions
are removed with the retired bodies. No V3 route is enabled.

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/woo/CLAUDE.md).
Composition: [exchange core contract](../safe-cex-exchange/CLAUDE.md).

The legacy response declarations in `types` retain their declared field types,
including optional-but-non-null `default_margin_mode`. They are declaration
models, not evidence that unchecked legacy Axios responses are validated.
The live declaration oracle produces missing/null/union fixtures consumed by
Rust tests. Run `yarn venue-oracle:woo` from the root to check source drift.

V3 migration contracts will remain inert; their literal-false migration gates
must not enable the new routes. Active legacy behavior remains separate.
The implementation is in progress; unimplemented rows remain pending.

`v3_contracts` contains inert declarations and migration metadata. The live
oracle pins all eight false gates, endpoint order and route ownership metadata.
Each gate has been independently flipped to true and rejected by a Rust test.

`journal` projects filled historical orders into one synthetic execution per
order. The production Exchange trait dispatches `fetchHistoricalPositions` to the
existing lifecycle body, with imports bound to that order adapter and the shared
`position_intervals`/`fold_position_adapter` implementations. The canonical interval
uses its native camelCase Serde bridge, retaining native numbers and optional-field
absence. It does not import journal grading or duplicate interval arithmetic.
`tests/binding_streams.rs` drives the constructed exchange through signed legacy
order reads, reported-PnL folding, missing-PnL/open/window refusal and runtime-clock
defaults. `scripts/journal-binding-controls.py` independently disconnects the trait,
each of the three imports and the interval field spelling; each compiled mutation
must fail the named production regression and pass after exact byte restoration.
Its `--interval-only` mode separately proves the canonical object guard using the
shared native bridge regression, including NaN, infinity, signed zero and absence.
See the [source adapter and its rationale](../../packages/safe-cex/src/journal/woo.position-adapter.ts)
and its [existing tests](../../packages/safe-cex/src/journal/woo.position-adapter.test.ts).

V3 string-literal enums deserialize through a string-only boundary. Private
remote derives keep serde variant metadata inspectable; the audit compares it
to live TS literal sets, including aliases/renames/skips, and rejects tagged
objects and other non-string inputs. The eight number|string siblings retain
both declared input kinds and reject unrelated JSON shapes.

Every deserializable record also has a map-only boundary. Private remote
companions provide serialization and deserialization from one canonical set of
wire field names. External qualified-call tests exercise `Type::deserialize`
as well as the trait path: adding a permissive inherent decoder must fail.

`v3_public` now contains the strict unknown-input public decoders and book/candle
mappers. These remain inert, with no live transport binding. Decimal validation
uses shared JS Number conversion, including radix prefixes and FEFF whitespace;
book normalization uses the shared sort and accumulator. Six new records are
map-only and have external qualified-call rejection coverage. Candle sorting
matches finite timestamps and stable ties. A declared NaN-comparator divergence
uses stable merge rather than Rust's total-order-assuming sort; live TypeScript
pins the engine-dependent permutation. Market mapping retains the filtered row index, validation order, original info
and leverage overflow. Futures-stat mapping uses shared Math.round. Prototype-property hits retain the
source row with absent id/symbol on output; declaration decoding still requires
non-null strings. Market lookup does not iterate the map, so output order is
the input row order. All V3 routes remain inert.

Indexed contract unions are resolved by the TypeScript checker, not reconstructed
from runtime tables: losing `as const` or adding a broad annotation must fail.
The contracts oracle exercises both in-memory widening controls on every run.

`api::sign_v3` prepares a per-request preimage and retains the original data
value independently. Raw JSON strings are signed quoted and remain strings for
the later adapter; do not replace one view with the other. Its current JSON
config boundary is declared CHOSEN and narrower than dynamic Axios config.
`api::sign_v1` uses the shared pinned root collator for sorted form queries,
including nested keys and repeated arrays. Factory adapter conversion is separately exercised through the real shared
HttpClient; standalone signer coverage does not claim live request dispatch.

Legacy fills uses `size`, `start_t`, `end_t` and `page`, preserving millisecond
units and present zero values. V3 query names are accepted-and-ignored by this
legacy endpoint. The query type is map-only and rejects explicit optional null.

Signing oracles resolve actual Axios, qs and create-hmac from the same isolated
installation as TypeScript in CI. The shared dependency generator copies the
root lockfile; do not substitute signing stubs or depend on root node_modules.
History projection propagates coercion/resolver failures and preserves zero-PnL
presence, executed price/quantity and timestamp units. Finite seconds overflow
is a preserved source bug, not a saturating conversion.
V3 signing now delegates truthiness, own-key order and JSON spelling to the shared
numeric primitives. The original HMAC boundary captures remain unchanged; all
eight signer controls were replayed against the shared-call implementation.

Symbol normalization and reverse spelling preserve separator precedence and
colon-tail fallback. Their rows disclose omitted lodash mutable memoization
caches. The response audit mutates the real caches and observes inherited results;
its native assertions run before source projection. Requested-symbol normalization
accepts a fallible live-method callback, retaining arbitrary result/error identity
and the empty-suffix call; the existing typed wrapper supplies the pure default. Hedge-side normalization uses the shared JS whitespace set, preserving
FEFF versus NEL behavior, and returns absence for unknown/one-way vocabulary.
Runtime package isolation requires absolute package paths: createRequire at the
compiler still searches ancestors. The mandatory isolation control tests absent
real qs/lodash with ambient copies present, plus a complete-install positive case.

`pricing` ports the pure IOC/slippage, touch-cap and post-only predicates used by
live WOO placement; it does not register placement or enable transport dispatch.
Fair-price selection order, NaN propagation and strict status/code comparisons
are source behavior. Error message coercion remains fallible and short-circuited;
the two JSON error predicates disclose the excluded dynamic JS object surface.

`response` extracts stateless private exchange helpers. Funding numbers reject
blank strings with JS trim; timestamps use Number and the source's 1e12 unit
threshold, retaining zero and signed zero. Message selection preserves padding.
The three numeric JSON boundaries disclose omitted native NaN/coercion-hook inputs;
symbol wrappers inherit the utilities' disclosed mutable-cache omission.

`response_class` supplies the two symbol method bodies installed as fresh own
arrows by `exchange_class`. The requested-symbol arrow captures its lexical
instance and performs a live transaction-normalizer lookup, retaining arbitrary
returned values and original method/getter errors. RegExp capabilities now take
pattern and flags, including global slash replacement. Two successful whole-class
constructions reach client assignment, both sockets and fill subscription before
native-first method assertions. The other 66 arrow slots and getPositionMode
remain unbound; this closes the two member bindings, not the owning-class FINDING.
The existing `response` wire facades retain their separate typed/cache boundaries.

The public `factory::create_api` composes the shared HttpClient and classified-read
retry engine with an explicit RateLimitClock. Fourteen real Axios/HMAC captures
exercise unsigned broker-failure return, remote-bypass ordering, array own broker
properties and version selection. The real HTTP recorder now compares complete
header maps (including default Accept), body bytes, URLs and timeouts, with zero
differences after integration's shared header/body repair. The former discrepancy
snapshot is removed. The manifest declares the narrower typed config surface and
integer-millisecond timeout boundary; this is not a dynamic Axios instance.
Options are an owned snapshot: a later caller secret mutation changes the TS
factory signature but not the Rust signature. A live source and recorder witness
pin both views under the shared owned-snapshot declaration.

The factory preserves core `RestBody::Transformed` provenance through signing
via `with_signed_data`. Rebuilding it as Untransformed makes the shared client
parse/trim a retried string again; a real-client recorder pins that phase boundary.

`map_woo_v3_markets` keeps owned `info`, unlike the source's live input-row alias.
The public oracle pins a later source relabel appearing through info while the
Rust witness retains its earlier snapshot; the manifest declares this choice.

`public_socket::ping` and `handle_ping_event` are callable heartbeat operations
on a core-owned BaseWebSocket. Ping checks disposal before recording monotonic
time and sends only afterwards, so a host send failure retains the recorded
time. Pong delegates disposal and readiness to core send. Forty actual-source
cases include base.send, missing/closed/open sockets and host errors; the clock
is observed through core latency reporting. This slice adds no socket lifecycle
registration and no Store writer.

The test-linked public socket dispatcher uses the shared parser, receive tracking
and core live-list callback dispatch. A 126-case oracle executes the actual
TypeScript onMessage and base stats method. Inherited handlers with nonzero
function arity reach stats and then throw; zero-arity functions and the prototype
object return first. Core now represents inherited handler reads and propagates immediate stats
errors before callbacks. It refuses global inherited-referent stats writes;
the live-source witness records that separate remaining boundary. The dispatcher
remains test-linked and pending, not a live socket registration or a parity claim.

Record constructors now capture through shared `JsNumberInput` before private
Serde companions, retaining the shared streaming map-only gate. Explicit
`JsNumberOutput` implementations register the whole output graph; generic V3
payloads use `JsNumberInput`, never `Value`. All eight number|string unions also
capture at their standalone public entry. The numeric corpus compares native
bits across raw, owned and borrowed routes, including lexical negative zero,
overflow and negative underflow. Strict V3 unknown-input validation functions
remain separate and retain their source rules.

`WooV3MappedOrderBookSnapshot` also captures through the shared numeric input,
using the shared `OrderBook`/`OrderBookOrders` output registrations. Its flattened
book keeps native numeric bits through bare Serde and shared entry APIs. The
private flattened companion independently rejects sequences; removing only the
outer map gate does not widen it. A public positional-decoder mutation exercises
the qualified-call rejection instead.

`orders` implements legacy limit/algo mapping over the production Store trait.
Shared get-key and TableHit preserve source lookup and branch decisions; algo
stop selection uses strict enum equality. Closed output fields refuse inherited
functions and the prototype object as whole-record errors, which differs from
TypeScript retaining the record. Dynamic copied numeric/bool fields are also
explicit typed-output refusals, not coercions. Open IDs use shared JS String.
Algo mapping completes the filter before mapping, skipping null/JSON-array
children. Closed-enum output refusals are deferred until batch completion, after
source ID/side coercions and clock calls; no successful subset escapes. Primitive
filter children use shared fallible `v_unknown`, preserving the exact source
in-operator message before any mapping clock. Dynamic
numeric/bool typed refusals remain eager and separately declared; this repair
does not claim those arithmetic/coercion paths. algo-timing.json pins the two,
one and zero clock paths and exact later coercion failures. Info is an owned parent snapshot; the live-source alias witness records
later parent changes separately. These pure mappers do not register the still
pending exchange runtime. Source captures precede the implementation and tag
inherited values before serialization can erase them.
Numeric mapper witnesses inspect 180 native field observations across 36 source
cases before JSON projection, including overflow, lexical negative zero and
negative underflow. They exercise shared JS numeric conversion directly rather
than passing through a typed-record Serde rebuffer.

Historical fee/rebate asset selection uses shared JS truthiness before shared
String conversion. An overflowing JSON numeric lexeme is truthy and becomes
Infinity or -Infinity in these open string fields; Number::as_f64's finite-only
projection must not erase the asset. Fourteen live-source cases pin both asset
fields across overflow, signed zero, underflow and ordinary numbers.

The three public ticker writers run through the production socket class. Their
87 source cases use the full production DefaultStore constructor and update
path, tagging numeric bits before serialization. The implementation uses native
ModelPatch numbers and drops the lookup borrow before writing; its Store result
is explicitly Result<(), StoreMutationError>, with the same explicit type in its
conversion through core ExchangeError::from. Fallible subscriber tests pin
commit-before-throw, ordered fanout abort and remaining-batch abort. Copied
non-numeric fields remain a separate typed-output divergence. Seven compiling mutations
pin field routing, filtering, batch continuation, deferred-refusal error priority
and signed-zero preservation;
real constructor/socket regressions additionally cover ticker, BBO and mark frames.

Typed writer reports are collected per refused row with its symbol, field and
original value, then deferred until batch completion, allowing a later source
TypeError to win with its own identity/text. The unrepresentable patch
and its notification are still lost (declared); other representable writes are
retained, and the operation never returns a successful subset.

Public ticker callbacks capture the canonical RetainedJsonInput directly. Native
number bits and UTF16 strings survive until row admission; ignored metadata cannot
reject a frame. Symbol normalization runs on UTF16 before the final typed Store
identifier check, so discarded surrogate segments do not reject valid targets.
The ordinary UTF8 symbol wrapper uses the same normalizer. Six real socket
regressions compiled and failed against master, covering all three public topics,
ignored metadata, overflow/signed zero, and normalized identifier admission.
Private timestamp callbacks retain their existing JSON input boundary.

`private_socket` extracts the private ping and timestamp helper. Ping keeps the
source sendWithLog wrapper: diagnostics are checked after sending, and heartbeat
classification stops diagnostic publication. Disposal prevents both timestamp
updates and sending. The timestamp helper uses shared Number coercion, rejects
nonpositive/nonfinite input, then floors with the source 1e12 unit threshold.
Its JSON input boundary excludes native NaN and executable coercion hooks.
Neither operation registers the pending private socket lifecycle.

The legacy fills interface inventory refuses heritage clauses before enumerating
local members. Its permanent actual-source inherited-number mutation must hit
that refusal, so adding a required inherited field cannot silently leave the
reported schema unchanged. Other schema member walks consume type literals;
class member extraction is a separate operation, not an interface census.

Field-identity supplements retain independent declaration fixture artifacts.
The legacy types artifact excludes the removed unused order DTOs; retained
cases stay byte-identical. Distinct samples address equal-valued field swaps;
separate recursive native-field assertions address symmetric Serde renames that
a roundtrip cannot see. FIELD_IDENTITY_AUDIT.md records the completed scope and
remaining inventories; a stronger declaration test does not imply the whole
venue audit is complete.

The public-writer numeric observer canonicalizes only NaN payload/sign at capture.
Signed zero, finite payloads and infinity signs remain bit-exact. New in-memory
supplements carry capture architecture; original fixture bytes remain untouched.

Contract identity checks read all 82 native fields across 12 records, with
explicit enum matches independent of Serde wire attributes. Mapper identity
uses a separate live pre-serialization numeric capture so JSON.stringify cannot
erase negative-zero output; the original artifacts remain unchanged.

`cancel_all_after` extracts only the inert limits, parity flags, validators,
builder and response decoder. It owns no timer or transport. Its JSON outputs
are explicitly mutable unlike the frozen source descriptors; both output type
rows stay pending. A live actual-source corpus compares numeric fields before
serialization, retaining signed zero and validation phase ordering. The module
uses shared numeric observation; no native Number parser or serde union is added.

`runtime_selector` extracts only inert route lookup, targets and selection using
the existing migration inventory. No transport, Store writer or gate override is
accepted. Unknown routes precede mode validation, modes precede lifecycle checks,
ownerless routes stay blocked and already-V3 reads are never shadowed. Plans are
owned mutable JSON, invalid object modes are snapshots and the route registry is
immutable; actual-source alias/freeze witnesses declare those separate choices.
The comparator and output declarations remain pending.

`transaction::parse_transaction_history_row` composes shared get-key, numeric
coercion and existing symbol/timestamp helpers. Nullish alias chains retain their
final null operand; timestamp aliases use truthiness instead. Price, quantity
and timestamp coercions all precede validation. The result retains native f64
fields and owned commissionAsset data; object aliasing and message-only errors
are explicit boundaries. The live oracle runs once per native test corpus.

The legacy history fetcher executes the supplied production RestAdapter once per
candidate, followed by the unqualified request. Empty successes retain the last
RestFailure unchanged; nonempty rows stop the loop. Native query numbers use
shared RetainedJsonInput, preserving signed zero and infinities while refusing
NaN before sending. Immutable options, owned returned arrays, typed failures and
response-transform parity are separate declared boundaries. Response phases
retain native data; only strings enter the parser. The factory can
supply the adapter; no exchange lifecycle registration is added by this helper.

`history_fetch` composes V3 candidate reads with the legacy fallback. An absent
or empty symbol takes one direct V3 request; legacy failure precedes a retained
primary failure. Primary parameters omit page and retain present zero bounds.
The adapter, ownership and response-transform boundaries match the legacy helper.

`historical_fills` composes the captured history callers with the real exchange
public-data check, REST adapter and fallible reporter. Parsing completes before
filtering. Direct fills sort stably by seconds; internal trades sort by milliseconds
and synthesize missing order IDs before the fallback seconds sort. Output asset
refusal occurs only after slicing and is separate from source-error reporting.
Nonfinite bounds are ignored before JSON conversion; a NaN limit retains the
existing shared parameter refusal. These functions do not register a WOO runtime.

The order-page source witness fixes request and output counts independently of
captured lengths. It observes copy-before-recursion, terminal array aliasing and
post-request disposal with fake xhr and reporter boundaries; native adoption is
separate and cannot inherit an Axios-transform claim from this capture.

The native page candidate iterates requests without a new retry or page cap.
It checks disposal after the request, copies rows before the next await and keeps
that prefix on a later reported failure. Boxed transport and reporter errors
retain their concrete types. JSON response and owned-array boundaries remain
separate from Axios transforms and source object aliasing.

`orders::is_algo_order` implements the legacy exchange predicate's three strict
OrderType comparisons. It neither coerces unknown values nor classifies
Conditional as an algo order. Its live oracle extracts the actual private
method and OrderType declaration, with six declared kinds plus ten fixed
missing/unknown/JSON inputs checked as native booleans. Placement and cancellation
registration remain on their own pending rows.

The page boundary supplement records metadata getter/coercion and row-iterator
events in order, with distinct first-failure identities. The native ordering
witness uses competing metadata-coercion and typed-row failures, paired with
valid metadata that reaches the row refusal; it does not model executable JS
getters. The serial page-control runner restores the source after every mutation
and records the starting SHA, source hash, measured exits and named assertions
under `/tmp/woo-pages-controls-evidence.json`.

Order-wrapper source preparation keeps mapper and fetch boundaries explicit.
Its identity checks cover every produced object, including indexed algo tails;
mutable-length and sparse input cases pin the source loops rather than assuming
array iteration or immutable input.

`v3_request` prepares inert V3 descriptors with shared qs-compatible sorted form
serialization, JS JSON spelling and shared HMAC signing. It never dispatches a
request or enables a migration gate. Typed input payload maps retain
`JsNumberInput` values; their opaque query values deliberately include more than
TypeScript's declared scalar/array query union. The raw preparation entry keeps
source validation order, then refuses an unsupported output method only after
body/signing phases. Prepared headers and descriptors are owned mutable values;
the source freezes both. Signing uses the existing asynchronous crypto seam.

`v3_ws_quotes` declares BBO/Trade records for the inert `v3_ticker` composer;
it adds no websocket lifecycle or decoder registration. `v3_ticker` combines the
existing futures-stat mapper with BBO bid/ask and Trade last, preserving validation
order, normalized identity checks and native numeric bits. Its owned info copies
BBO/Trade, while source info keeps their live referents. Its ticker/info records
are mutable unlike the source's frozen objects. All seven new record decoders
capture numeric input before private map-only Serde companions. The focused
source/native target covers all twelve new symbols, with one compiling control
per family; it performs native field assertions without roundtrip comparisons.

`decode_woo_v3_regular_place_response_with` accepts a fallible live Set membership
call after order-ID validation and before later response fields. Its generic
Membership error retains the caller's original value; the existing slice facade
delegates to it with ordinary membership. Native decoded fields, callback counts
and error identity precede five actual-source cases, ending at wrong-request
refusal. Captured response/output ownership remains a separately declared cost.

The regular mutation decoder returns a validation error for a status outside its
closed native domain, even when a public caller includes it in the allowed slice.
The BAD-status witness first reproduced the old unreachable panic. This is a
native API safeguard; BAD is outside the TypeScript status declaration.

`decode_woo_v3_algo_place_response_with` decodes every row before calling fallible
known-ID membership. Each call precedes internal duplicate detection, and the
whole loop precedes positional blocking. Its generic Membership error retains
the caller's original error; the existing slice facade delegates to this body.
The caller supplies live Set method lookup and receiver invocation. Nine source
cases end at a changing has getter; native fields, ordered calls and error
identity are asserted first. Owned/mutable captured response boundaries remain
CHOSEN and are observed separately from source shallow freezes and nested aliases.

The account leverage audit observes the existing independent native query/target
snapshots, mutable typed results and concrete gateway errors against actual
source aliases/freezes and spread extras. Reversed hedge details retain native
LONG/SHORT leverage routing before source projection. Separately supplied native
request/target arguments have a broader construction domain than source ready
targets; this remains a declared API choice. Eight account rows retain CHOSEN
with explicit interface rationale; no new dense-data behavior defect was found.
Lane-wide discovery counts and row identities live in DIVERGENCE_AUDIT_LEDGER.md.


The error/validator audit supplements the existing pricing/account corpora with
native-first assertions and actual-source `--guard-audit` modes. Its eight-row
floor stops at requireExactKeys. Four string/key guards have MIXED notes backed
by materialized D800 observations; native scalar/map refusals are whole-value
outcomes. Captured list methods and owned record/error interfaces remain priced
choices. DIVERGENCE_AUDIT_LEDGER.md separates those four from choice-only and
behavior-code discoveries; they must not inflate either binary subtotal.


Position-mode evidence has a native-first `--choice-audit` supplement. It observes
matching D800 reconciliation IDs reaching Ready and a D801 mismatch blocking;
native shared/typed ingress refuses the whole input. The ledger revisits the
already-counted complete-evidence declaration without adding another identity.
Captured arrays/regexes omit live every/test overrides and sparse-hole behavior;
these choices are priced separately from the four programmatic-ID MIXED rows.
The result declaration proof remains limited to builder-produced variants.


Algo request builders have native-first `--request-audit` observations. Common
and place outputs retain D800 tags in source, and amend compares a stored D800
decimal; native whole-input refusal is explicit. Target ownership, TP/SL order,
hedge close sides and singular/plural cancel routes are separately pinned.
The ledger reports both cumulative code-bearing rows and the latest marginal
rate; capability/representation findings do not inflate behavior-code discoveries.


The algo guard/metadata supplement runs native assertions before --guard-audit.
Its eight divergent rows include the policy constant and seven guards; the
already-ported trigger predicate stays an extra observation. Record/array/String/
allowed-enum outputs retain D800 in source and have explicit whole-input native
refusals. Ownership, live-method and fixed-intrinsic costs remain separately priced.
The ledger tracks cumulative discoveries and each zero-code cohort independently.

The first accepted-set batch reviews 32 V3 string enum owners independently,
including every declared tag and tagged-object refusal through four native
entry paths. Twenty older derived enums now use the same String admission and
private remote schema as `v3_contracts`. See
[accepted contract review and focused commands](ACCEPTED_CONTRACT_REVIEW.md).
The focused runner retains full compiler discovery and selected-source scope
checks; it does not classify the remaining Woo owners or aliases.

The next accepted-set batch reviews 27 retained concrete declaration records, with
native views of all fields and separate raw/owned/borrowed/qualified ingress.
The V3 public mapper's five numeric validations are expanded from their former
local macro so source discovery can inspect ordinary Rust. Its existing typed
candle-sort helper retains its reviewed local role and exact source pin. See
[record review and focused controls](ACCEPTED_RECORD_REVIEW.md).

The [validator contract review](ACCEPTED_VALIDATOR_REVIEW.md) documents 30
additional exact free-function APIs, native bit/presence/borrow observations and
their independent live TypeScript corpora. Its focused runner retains full
compiler/source preflight and supplies copied-source controls; it does not claim
whole-package accepted-set clearance.

Live `mapLimitOrder` and `mapAlgoOrder` return runtime values in
`binding/hosts/order_mapping`; the old typed mapper functions and their
facade-only tests are removed. The real class preserves own undefined table
hits, copied amount/reduceOnly/filled values and original parent info aliases.
Order lookup tables retain their actual shared Object.prototype referents.
The 103 source records, 36 native numeric-bit witnesses and 12 source timing
cases now run through production class members. A real HTTP algo read keeps a
partial STOP plus its two bracket peers instead of discarding all three.

`scripts/algo-order-control.py` compiles a closed-output regression in the live
mapper, requires named mapper and HTTP failures, restores exact bytes and reruns
the native suite. `scripts/terminal-report-control.py` separately restores a
closed-output refusal inside the real limit mapper and verifies the three
terminal private-frame regressions. Both source scripts execute actual TS bodies.
NEW/PARTIAL Store insertion and public typed results retain the separate shared
Order field-domain obligations. This repair makes no shared-type or nonnumeric
Store insertion claim; it neither coerces copied strings nor drops bad rows.

The unused `WooxOrder`, `FetchOrderResponse` and nested meta DTOs are removed.
Their strict declaration decoders rejected documented null metadata even though
production order reads already consume raw runtime rows. The legacy declaration
oracle and accepted-record cohort exclude these erased-only records; remaining
record guards, field and numeric witnesses stay active. `binding_orders` exercises
all four documented null fields through the real signed HTTP/class mapper and
retains their own null keys in info. `scripts/legacy-order-null-control.py`
compiles a live-mapper null-refusal control that must fail its named order-count
assertion; decoder-only tests are not evidence
for this production path.

The production WOO signer now receives and returns the original native runtime
config through `api` and `factory`. Shallow copies retain nested data identity,
callables and own properties. V1 reuses the shared runtime qs serializer; V3 uses
the existing native JSON.stringify operation at its signing boundary, retaining
source BigInt throws and toJSON callbacks before any data-only transport capture. The constructed
exchange regression in `native_signing_retention` observes NaN and Infinity in
actual HMAC preimages before the terminal wire projection. Finite JSON fixture
views in older tests are explicit observations after signing.

The configured fetchOrders member admits mapped rows individually before either
the Exchange trait or startup Store write. Order.amount/filled must be numbers;
reduceOnly admits absent, own undefined and booleans. A malformed row emits its
id, field and original typed value through emitAndLogError and is omitted; other
rows keep their values and presence. This is an intentional accepted divergence
from TypeScript retaining malformed typed fields, per shared-type decision 3.
The raw mapAlgoOrder/fetchAlgoOrders source-value witnesses remain unchanged.
The real mixed-row binding regression covers amount, filled and reduceOnly via
both the public read and actual configured fetch-to-Store path.


The constructed clientOrderIdRule retains its source Number(input) conversion and
uses the shared noncoercing Number.isSafeInteger builtin. The actual-rule witness
compares26 maintained-source identifiers, including safe-range and canonical-decimal
boundaries, without replacing the rule or its operation table.

The live order-page reader passes truthy message/meta.message values directly to
the shared Raw(Js) reporter. Non-string errors must retain completed pages;
`binding_orders` covers actual signed pagination, original emitted text and fallbacks.

The constructed ticker census in `tests/binding_streams.rs::production_ticker_coercion_matches_constructed_typescript`
uses the maintained TS exchange and DefaultStore via `parity/scripts/ticker-strict-source.cjs`.
72 packed three-row frames cover every copied ticker/BBO/mark numeric field. Fifty-four malformed rows are visibly refused while their good siblings retain source-equal native values.
The corpus includes null, strings, NaN-producing inputs, objects, booleans, missing
fields, raw numeric overflow and signed zero. It compares all eleven native number
fields before JSON; approved decision-3 refusal preserves the entire bad row.
`parity/scripts/ticker-native-controls.py` separately disables mapping and refusal
behavior, requiring a compiled native failure and exact restored pass.

Guard/algorithm/position-mode native corpora admit canonical retained UTF-16 inputs
and inspect their units before strict String/DTO projection. Original typed field,
enum, source-case, numeric-bit and effect assertions remain unchanged.

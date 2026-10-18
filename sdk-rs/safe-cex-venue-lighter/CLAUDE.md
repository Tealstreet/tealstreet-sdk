# Lighter Rust venue

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/lighter/CLAUDE.md).
Shared runtime contract: [exchange core](../safe-cex-exchange/CLAUDE.md).

Protocol tables in `src/types.rs` preserve source order and all deployment overlays.
The `LighterDeploymentProfile` inline quote union is a separate Rust enum; required
fields reject missing/null values. Cow strings permit both constants and owned reads.

`yarn venue-oracle:lighter` runs the live TypeScript comparison, including ignored
Cargo tests. Set `SAFE_CEX_TYPESCRIPT_PATH` to an isolated compiler when needed.
The oracle evaluates the worktree's TS and actual domain enums, never a copied snapshot.
Keep table keys/values, union alternatives, and profile fields covered when extending it.

Deployment helpers take the shared realm's `TransportRegistry` explicitly, looking
up its default provider on every call. Raw frame dispatch preserves TS substring
matching, including whitespace sensitivity; do not replace it with JSON parsing.
The pure oracle tags nonfinite numbers and negative zero to avoid JSON erasure.

Offline signing evidence lives in `tests/fixtures/signing-preimages.json`, maintained
by `scripts/capture-signing.cjs` and checked by the mandatory venue oracle. It
captures both signer and TransactionClient boundaries, real WASM preimages and
outputs, and serialized wire requests before Rust signing integration. Both source
packages call themselves 0.0.1: the fixture's source and WASM SHA256s are the real
version pins. Regeneration uses only the public test key and fixed time/entropy.

Earlier SDK scalar/common/order records use Value-first ingress, which retains
raw overflow lexemes under arbitrary_precision; their direct typed raw-serde
boundaries remain separately measured. This is not a blanket SDK contract:
`sdk_responses` and the four adopted venue records capture JsNumberInput inside
public Deserialize and preserve numeric bits on bare raw/owned/borrowed paths
as well as shared entries. Generic wrappers retain their separate legacy
bare-Value FINDING, as detailed below. Audit each entry path rather than
inferring its behavior from another record's implementation.
Numeric SDK enums accept number-typed values beyond named members, so their
newtypes preserve all binary64 values; indexed-access literal aliases stay closed.

Generic SDK response wrappers preserve optional presence independently of nullable
payloads: `ApiResponse<Option<T>>` retains `data: null` as `Some(None)`, while a
missing key remains `None`. Payloads need Deserialize, not Default. The private
remote companions sit behind the strict object guard, including qualified calls.

Canonical generic ingress uses shared `js_from_str`, `js_from_value` or
`js_from_value_ref` with `ApiResponse<JsNumberInput>` or
`PaginatedResponse<JsNumberInput>`, followed by payload `decode`. Explicit
`JsNumberOutput` implementations exhaustively register fields. Legacy direct
serde decoding with a Value payload still has a separately diagnosed lexical
negative-zero loss; it is not the canonical ingress contract. Numeric metadata
and retained payloads have raw/owned/borrowed bit witnesses, including overflow
and negative underflow. This registration does not rewrite generic Deserialize:
its legacy Value payload loss stays a FINDING until a production SDK consumer
selects the retained-payload entry API. Retained strings keep the shared UTF8 restriction.

Approval fee caps, cancellation targets and order minimums use JsNumberInput
inside their private map-only decoders. They also register their full output
field graphs for shared whole-entry ingress. DeploymentProfile uses the same
buffer after adopting shared Cow<str> eligibility, without changing its borrowed constants or owned decoded URL fields.
The eight-field audit covers both bare serde and shared entry APIs on
raw/owned/borrowed and nested input paths; no clean SDK result is generalized to an older decoder.

SDK cancellation/fill-query request records do not impose validation absent from
the source interfaces: cancellation IDs remain optional and numeric fields stay
binary64, including non-finite values. Optional non-null fields reject explicit
null; no-default required statistics fields and public map-only decoding are
checked against the live orders.ts schema. These records retain the shared UTF8
string boundary, with its own positive Unicode and refusal witnesses.

OrderInfo and Fill use SDK numeric enum newtypes, not the venue's closed order
side/type enums: the SDK source accepts number-typed values outside named enum
members. Order-detail oracles test numeric payload bits, distinct string fields,
and each boolean independently; equal fixture values must not hide field swaps.
Compiler-derived public field type names also pin enum identity: equal numeric
representations do not make OrderType and OrderStatus interchangeable. Validate
the live source schema before decoding a fixture so a source mutation fails at
its named schema assertion rather than an unrelated decoding error.

SDK create/modify/query parameter records mirror the declared interfaces, without
adding price, quantity or index validation. QueryOrdersParams accepts an empty
object because every field is optional. Open numeric SDK enums and optional
non-null booleans keep the same decoders used by order details; named accepted
field-type checks protect both required and optional wrappers.

Batch cancellation ID lists preserve order and duplicates, including adjacent
duplicates; empty lists are distinct from absent lists. No deduplication or
nonempty-ID validation belongs in these interface records. Nested string-array
items share the UTF8 boundary, and the live corpus checks empty strings as well
as scalar/astral/whitespace and lone-surrogate cases.

SDK result, nonce, API-key, transaction-hash, server-time and rate-limit records
use shared numeric ingress from their first implementation. A numeric source
field stays binary64 even when its name suggests an integer, timestamp or bitmap;
no positivity, integrality or range guard is added to a TypeScript interface.
Private map-only companions preserve optional non-null fields. Compiler-derived
field identities distinguish ResultCode from numerically identical SDK wrappers.

Deposit, transfer-history, maintenance-window, fee-tier and funding-rate SDK
records use the same public JsNumberInput decoder. Amounts and fee/rate strings
remain strings; timestamp, interval, confirmation and tier numbers stay binary64.
Maintenance affectedServices preserves its list order, duplicates and empty list;
optional memo is absent or a non-null string, including an empty string.

AccountApiKeys and ExchangeInfo retain typed child arrays, including order and
adjacent duplicates. Numeric ingress witnesses cross the outer container and the
child decoder on bare/shared raw, owned and borrowed entries. Nested fields use
the SDK ApiKeyInfo and RateLimit records, not Value payloads; child strings retain
the same explicitly witnessed UTF8 restriction as their standalone forms.

ErrorResponse, FeeSchedule and InsuranceFund use the shared JsObjectMap for
string dictionaries. Integer-like keys follow JS enumeration order; other keys
retain insertion order, including own prototype-named keys from JSON.parse.
Optional dictionaries reject null while retaining empty objects. No local
ordered-map wrapper or numeric eligibility substitute is needed. Distinct
dictionary and fee-tier field values prevent a swapped-field false green.

Generic retained-payload ingress also keeps the shared recursion cap. This is a
separate CHOSEN acceptance cost: Node and legacy owned/borrowed Value payloads
accept 140 nested arrays, while checked raw/owned/borrowed entries refuse them.
Boundary witnesses retain up to 126 payload arrays for ApiResponse and 125 for
PaginatedResponse, whose items list consumes an additional level. The legacy
numeric FINDING remains independent; a depth refusal is not its repair.

HealthResponse and ComponentStatus keep closed status unions separate from open
strings. Their public union decoders require a string before private serde
companions; accepted-variant metadata stays visible so aliases cannot silently
widen the set. Qualified-call rejected-object controls protect against public
permissive decoder bypasses, alongside exhaustive variant and field-type checks.
Optional connection statuses distinguish absence from explicit null.

Account information, withdrawal/deposit histories and system status retain their
source literal unions. KYC absence differs from the literal "none" and from
rejected null. SystemStatus carries typed ComponentStatus and MaintenanceWindow
arrays, preserving empty lists, order and duplicates; nested fixtures keep child
union values valid while varying free strings, numbers and string-array items.

SDK transaction request records are interface shapes, not transaction validators.
Nonce, expiry, indexes, gas limit and signature v remain binary64; no integer or
range restriction is inferred from comments. Optional gas price, transaction
flags and identifiers reject explicit null while preserving absence. Create
requests retain the SDK's numeric OrderType, OrderSide and TimeInForce wrappers.
Public decoding captures JsNumberInput before a private strict-map companion;
the transaction oracle checks bare/shared raw, owned and borrowed numeric bits,
field identities and shared UTF8 string refusal witnesses before integration.

Each L2 transaction info record has its own numeric-literal discriminator type.
The general TxTypeValue union would accept the wrong transaction kind and is not
used for these fields. Live checker assertions require a single NumberLiteral,
so removing TxType's as-const or widening the field cannot silently widen the
oracle. Nested TransactOpts uses the same checked decoder as standalone options;
both numeric and optional gas-price witnesses cross the outer record boundary.

Whole-record capture has an independent CHOSEN depth cost even when the deep
value is an unknown field that serde would discard. The maintained audit covers
all 41 capture-based records: 22 SDK responses, 15 transaction shapes and the
four older venue records. Root extra fields accept 126 nested arrays and refuse
127; extra inside L2 opts accepts 125 because opts adds a level. Node and ordinary
serde ignoring unknown fields accept 140. We retain the shared bounded-capture
policy rather than fork unbounded numeric ingress locally; Rust can express the
alternative, so this is not FORCED. UTF8 and numeric-repair notes remain separate.
The same unknown-field capture also imposes the shared UTF8 boundary on otherwise
numeric-only records: lone-surrogate extra strings are valid JSON accepted by
Node and skipped by ordinary serde, but refused by bare/shared raw capture.
Positive scalar/astral/BOM/NEL extras remain accepted. These are raw-path witnesses;
serde Value cannot carry the unpaired UTF16 input for owned/borrowed tests.

SDK interface censuses refuse heritage clauses before walking members. Each of
the five censuses has an inherited-required-number mutation checked by name at
Rust exit 101; inherited fields must never silently disappear from the schema.

L2TxInfo is an untagged union of eight existing literal-discriminated records.
Its private decoder stays behind JsNumberInput; every alternative is checked
through six numeric entry paths because serde's untagged buffering is itself a
possible numeric-loss boundary. Unknown-field depth and UTF8 costs remain
independent of the retained tag and numeric-bit claims.

Identity checks must read decoded native fields; re-serialization can cancel a
symmetric serde-name error. Common SDK record tests vary one leaf at a time and
project native fields recursively, including Account balances and OrderBook
levels. Keep edge-value corpora even when their repeated values are intentional.

SignedTx preserves the typed transaction union and signature rather than an
opaque payload. Native field checks cross this outer boundary independently of
inner record tests. Unknown extras consume capture depth even when discarded:
root 126, tx/signature 125, and tx.opts 124 arrays are accepted. These shared
policy costs and unknown-string UTF8 refusals remain explicitly CHOSEN.

NaN capture tags are semantic; future powf input-bit captures canonicalize only
NaN payloads. Signed zero, finite values and infinity remain bit-exact, with
separate zero-flattening and finite-corruption controls on the capture helper.

Generic identity fixtures originate in the Node oracle, never from Rust Serialize.
Read every decoded wrapper field directly; distinct metadata and payload values
plus all boolean combinations prevent both fixture and symmetric-name blindness.

Order-record identity checks read each native field and vary one field at a time,
including optional booleans and ID arrays. Serializing a whole record remains
a shape check and cannot substitute for these field-identity assertions.

Response identity checks recurse through typed child arrays and map closed enums
through exhaustive native matches. The response test process captures its Node
corpus once; changed-source controls retain independent child processes.

Transaction leaf identity checks recurse through opts independently of union and
SignedTx checks. Transaction and generic-wrapper processes each capture their
source corpus once; source-mutation children capture their own changed corpus.

The signing field-identity supplement is a later live gate with distinct fee
values and direct preimage-word assertions. Generated JSON stays ignored; the
original shared92-case fixture is immutable. Shared crypto adoption at 74aa2775f1
pins the ab2aadb8f5 generator; subsequent local headers record that adoption
without relabeling the archived capture or claiming integration/CI execution.

Signer and approval type-only records have distinct native construction/Clone
witnesses, with all create-order boolean combinations. These are not claims
about unported request decoding or signing composition.

The older numeric-ingress audit batches all 104 numeric and Unicode witnesses in
one Node capture per process. Unknown-depth corpora remain separate; raw lexical
negative zero and escaped UTF16 witnesses are retained in the batch.

Deployment-profile decode identity reads all seven native fields and exercises
all four proxy-flag combinations independently of the constant-table serializer.

The pure-helper gate reads native CancelTarget, OrderMinimums and approval-cap
fields from distinct inputs before retaining its same-value domain roundtrips.

Utility result checks read minimum amounts and cancel target IDs directly before
serialization. Distinct snake/camel minimum inputs supplement the error corpus.

Close-classification records assert all native fields both after classification
and after decoding all public/private boolean combinations; rejecting incomplete
records alone is not a successful-decode identity check.

The shared Store notification cutover is adopted from integration 9419b340d8a6.
No current Rust helper/type row writes Store; pending runtime writers are listed
in STORE_NOTIFICATION_AUDIT.md and must preserve structured mutation failures.

Integration 9af82bfeae adds structural subscriber IDs to ExchangeErrorMetadata.
Both shared mutation-error conversion branches retain inner-to-outer IDs; these
are Store-local, not globally unique. Lighter has no local error boxes to replace.

Cardinality assertions must independently pin source corpus totals and native
output lengths before pairwise comparisons. Whole-array equality sees extra
outputs but cannot prove a nonempty oracle corpus was visited; keep both guards.

Collection cardinality has a separate consuming-decoder audit. The response
arrays cross six bare/shared entries; common Account/OrderBook and batch-cancel
arrays currently cross three bare entries, while paginated items cross six.
The later Node supplement pins source array types and distinct items without
rewriting historical fixtures. Native identities, fixed lengths and executed
comparison totals remain separate assertions.

`scripts/check-consuming-collections.py --exclusive-worktree` temporarily
mutates decoder inputs, runs scoped controls serially and restores exact bytes.
Run it only with exclusive access to this worktree and no other Cargo process.
A non-consuming trailing-element rejection is not accepted as mutation proof.

TxReceipt retains optional data as JsNumberInput: absence differs from present
null, and numeric payload lexemes survive checked decoding. This JSON carrier
is narrower than the source any field; undefined, bigint, function and symbol
values remain a declared finding. The closed TxTypeValue tag and optional
non-null metadata are checked independently of opaque payload acceptance.

SDK batch unions retain the original JsNumberInput after membership validation.
TypeScript structural unions have no runtime first-match dispatch; overlapping
order/transfer shapes must not lose fields through a selected Rust variant.

Batch response errors use an ordered string dictionary. This is narrower than
Record<number, string>, whose nonnumeric extra keys can carry other types; that
restriction is a separate FINDING, not a numeric-ingress or UTF8 claim.

Batch union extras also retain a JSON-only domain: source own undefined, bigint,
function and symbol extras have separate checker/typeof witnesses. Batch gates
pin full fixture content and read native fields; symmetric serde names are not
validated by roundtrip bytes. The consuming-control runner retains collection
identity when count guards are removed, and reports which assertion catches it.

Checker-based oracles must reject compiler diagnostics before publishing a
capture. The older alias/transaction oracles check after their existing checker
walk, preserving historical literal-union output order and capture bytes.

SDK order API records keep the numeric OrderStatus wrapper and distinguish all
three flags independently. AccountOrdersResponse retains ordered typed orders,
optional non-null total and cursor; no OrderApi request method is adopted here.

The order API oracle compiles selected byte-exact source interfaces and their
actual OrderStatus import, not the Axios/client runtime closure. Diagnostics
must be empty, and each interface heritage guard has its own source control.
Its live corpus digest excludes only runtime metadata and protects encoded
inputs as well as labels/counts; replacement and reversal controls exercise it.

Encoded-surrogate controls replace one escaped lone UTF16 unit with another,
asserting changed bytes and unchanged encoded length before the corpus assertion.
An unrelated malformed JSON object does not establish this control.

Order-book API response arrays use named inline child records and the existing
SDK OrderBookLevel. Native projections read every child field before wire shape
checks; corpus reordering must change content (reversing a palindromic fixture
is vacuous). Numeric input and unknown extras cross six bare/shared entry paths;
root, inline child and nested level extra limits are 126, 124 and 122 arrays.

Pure TransactionClient helpers accept UTF16 slices so prefix removal and dynamic
range-error names preserve unpaired units. UTF8 conversion uses TextEncoder's
replacement semantics. Native encoding is always available; the TS ambient
no-TextEncoder/no-Buffer failure is a separate CHOSEN runtime difference.
Uint32 fields occupy eight little-endian bytes with four zero high bytes;
market order type overrides every post-only/time-in-force setting.

Unsigned TransactionClient builders retain bigint/UTF16 parameter values without
inventing a JSON codec for bigint interfaces. Their injected clock is evaluated
at the original lazy Date.now sites; create metadata and decoded preimage words
are checked before returned hex equality. Transfer keeps its legacy minimum-width
radix-16 text, including invalid-for-signing values. Group validation checks
same-market before duplicate ID; composite faults pin that precedence. Grouped
crypto aggregation, class state, nonce allocation, signing and HTTP stay separate.

Native signing composition now lives in `transaction_signing.rs`: regular signing passes the original payload to Schnorr, attributed signing extracts the unbound fp5 callback once, and grouped construction builds every child before hashing and folds hashes in order. `http_transactions.rs` supplies the ten value-level transaction DTOs with UTF16 strings and unbounded BigInt. Auth-token composition is synchronous and therefore CHOSEN divergent from Promise scheduling; the class, nonce scheduler, HTTP and WASM lifecycle remain pending. Live source tests witness native values and callback arguments before trace comparison; deterministic probes do not establish cryptographic arithmetic parity.

`transaction_client.rs` now provides native class/nonce/get/submit orchestration over `transaction_promise.rs` FIFO microtasks. Hosts must enqueue continuations and never execute inline. Promise primitives are readonly Rc values; mutable DTOs and arrays use ObjectPromise/RefCell and retain fulfillment identity. Nonce lock release precedes the result settlement, allowing a queued allocator before its preceding caller builds. Dropping the returned handle does not cancel work. Constructor, nonce allocation and reset are ported; class plus fourteen get/submit wrappers are CHOSEN divergent for owned input snapshots versus mutable JavaScript caller aliases. Auth now uses eager settled/rejected promises and lazy encoder availability after first readiness. Native batch response txHashes aliases are retained. Actual HTTP, crypto, JS promise interop, thenable assimilation and unhandled rejection hooks remain outside this host boundary.


SDK primitives slice: `sdk_validation` mirrors source validation branches and repeated getter reads using explicit JS primitive tags, unbounded BigInt, UTF16 strings and identity-preserving host throws; deadline invokes the host clock before converting the declared numeric minimum-future argument. The native ValidationError class/constructor are FORCED divergent for JS prototype/reflection/stack and lossy Display, with exact native name/message units retained. `http_models` maps nine value-level declarations using raw nonce/fee strings and unrestricted generic payloads; no HTTP decoding, aliases or property enumeration claim. `http_number_helpers` maps the two pure bigint safe-integer helpers; api-client HTTP methods and hex/base64 conversion remain pending. `live_sdk_primitives` has 310 scalar, 44 getter, 27 HTTP record, three Error-field and nine bigint-helper oracle witnesses. HtmlDda has a native marker but no browser oracle proof.


Account SDK slice: `sdk_account_api` maps all13 AccountApi value declarations as UTF16/full-f64 native values with generic metadata; methods retain getter ordering, two reads for included options and successful data, opaque throws, eager work and exact failure fallbacks. The caller must implement AccountSemantics ECMAScript property/coercion/Promise Resolve operations, including thenable assimilation, plus FIFO TransactionRuntime microtasks; these are required host contracts, not a standalone JS engine. AccountApi class is CHOSEN divergent for explicit native host context/prototype/reflection surface; constructor handle retention and seven bodies are ported at those boundaries. `live_account_api` witnesses179 runtime scenarios and39 native records; direct fields/identity assertions precede the independently pinned source capture. No HTTP decoding/transport or exhaustive host Promise Resolve proof is claimed.


Transaction history SDK slice: `sdk_transaction_api` maps two string enums,11 native value declarations, constructor handle retention and six eager query methods. It retains three own query keys even when undefined, separate types/filter getter reads, bound join on the third types value and raw UTF16 hashes/JS binary64 formatting. Shared AccountApi request_projected performs detail data.transaction projection in the same await continuation before host return resolution; identity projection preserves account behavior. TransactionQuerySemantics supplies abstract greater-than-zero comparison and receiver-bound join; getters/coercion/FIFO/Promise Resolve remain required host contracts. TransactionApi class surface is CHOSEN divergent; bodies/declarations are ported within those contexts. New oracle floor192 runtime/45 record witnesses plus3/5/4 variants; account regression oracle remains179/39. No standalone JS engine or exhaustive host comparison/join/resolution proof is claimed.


Ordinary order-query slice: sdk_order_queries maps seven pending native value declarations, constructor client retention and seven ordinary method bodies. It preserves limit||50, repeated option and tuple-container/index reads, raw cursor/sort second values and required receiver-bound toString operations with non-string returns/throws. Inherited ApiTrade coverage checks actual Trade import binding, base schema and derived redeclarations. Partial OrderApi class and raw getOrderBookOrders remain pending; request-config spread/precedence and raw code200/array/clock/error behavior need a separate native ABI. New native-value types do not adopt or change existing wire DTO divergences. Oracle floor251 runtime/28 records; required ambient host contracts are not a standalone JS engine.

Market-data SDK slice: `sdk_market_api.rs` maps CandlestickApi, FundingApi and InfoApi plus eight value-level DTOs. Methods use `AccountApi::request_handled` so response checks and returning data occur in the original await continuation. `MarketApiErrors` is a required source LighterApiError constructor host: evaluate message and code first (including two separate response.error accesses), then construct/coerce. It is not itself a native implementation of the still-pending LighterApiError class. Native classes require explicit runtime/semantics hosts; prototype/reflection is CHOSEN divergent. `live_market_api` uses the actual TypeScript source oracle, asserts native values/identity/query fields/read order before the authored corpus pin, and stops at 120 method cases and 32 model cases. No network or JSON codec claim.

Public ingress bodies use `public_ingress::IngressOps` with live opaque values and mandatory host operations. See `PUBLIC_INGRESS_EVIDENCE.md` for the nine-row mapping, iterator/callback contracts and finite proof scope.

Production `binding::exchange::LighterNativeExchange::new` assembles the ported
class on the shared js_runtime over `LighterDependencies` (socket/runtime, HTTP,
transport registry and optional signing entropy). Leave `signing_entropy` absent
for the host CSPRNG; deterministic sources are for scripted transport proofs.
The typed Exchange surface retains the five inherited BaseExchange rejections.
`tests/exchange_binding.rs` drives all sixteen methods plus batch placement;
`scripts/binding-controls.py` proves every binding with a compiling mutation and
restored pass. Run the control script exclusively, without another Cargo process.
Builder-fee options come from `options.extra`, with the exchange name as the
second argument; invalid present overrides must not select the default index.
Signer validation has a separate nine-case live TransactionClient precedence
oracle. That bounded proof does not establish arbitrary SDK input, constructor
reflection or typed-array crypto parity. The archived signing-preimage fixture
remains immutable; source-only corpus provenance is checked after native values.

The current source Lighter family uses LighterApiClient and a local TransactionClient
signer. It does not populate BaseExchange.xhr for shared hosted HTTP signing;
ensureSignerAdapter and SDK initialization require a local secret even when a
remoteSigner is available. `tests/hosted_signer.rs` compares six live source gates
and both native family names with an advertised hosted capability, preserving
the missing-key error before HTTP or signer calls. This evidence covers that
conditional source limitation, not hosted transaction signing.


Native stream, candle, fills and private snapshot hooks now invoke the existing
Lighter class members. Listener cleanup calls the original closure once; it
must remove the parser or polling timer. The snapshot ignores the base retry
argument because the source helper owns one attempt and its error handling.
Historical fills retain the source public-only refusal, authenticated reads,
filtering and complete collection; they no longer hit the empty trait default.
`real_trait_` regressions in exchange_binding use the production constructor,
actual signed HTTP and socket events. `scripts/trait-binding-controls.py` requires
compiled named failures and exact restoration, including empty/subset REST
results and both listener cleanup paths. Run controls without another Cargo
process or concurrent production edits in this worktree.

Measured stream/history checkpoint: the six regressions fail on the original
trait defaults and pass after binding; the complete constructed target passes
23 tests. Six independent compiled production faults fail named tests, and each
exact restoration passes all six regressions. Venue-local parity passes 695
manifest rows and 1,404 public Rust items with full workspace target resolution.

Strict host all-target and WASM library lint pass. The override contract passes
8 tests including the live TypeScript inventory, and the production binding
inventory passes 25 tests. After the shared ModelField and library-pin merges,
all 23 constructed tests and the all-features cancellation inventory (6 tests,
explicit export ignored) pass.

Private feed/stream for-of and spread consume the shared GetIterator protocol,
including iterator objects and live collection mutations. A for-of body throw
runs IteratorClose while preserving its original cause; next/done/value failures
escape without closing. Registered private-class regressions cover live Set,
Map-values and array traces plus these four abrupt-completion boundaries.
This slice is scoped to binding/ws_private/slots.rs, rather than every venue host.
The supplementary close-spread regression substitutes an iterator-valued helper
result and restores the real DefaultStore mutator. On the master slot body the
position remains open; the repaired spread delivers the close pair, clears margin
through own undefined, and increments the position revision once. It exercises
latent iterable admission; maintained wire payloads still provide arrays.

Partner-attribution order rejection helpers are installed in the production
module globals. Their source String/nullish coercion runs before the existing
account-tier classifiers; matching codes short-circuit the message read.
Real signed-placement rejection tests cover tier, approval and ordinary errors,
including the normalized message delivered through the native error reporter.

Public ingress loops and pair patterns use the existing native IteratorRecord.
Abrupt callbacks and early breaks invoke return once; original callback errors
win over close errors, while normal pair completion validates the return result.
The constructed public class and its real operation table carry the regressions.
The native exchange target also covers account_all orders, positions, fills and
the source-throttled balance refresh through the real private socket.

Account-tier extraction consumes live Js records, stopping at the first non-nullish
alias even when its normalized tier is unknown. It preserves getter order, native
number values, UTF-16 strings, unrelated cycles and original throws. The attributed
placement regression drives overflow-number limits through the real client and
signer, while live_tier retains the existing source corpus plus native-value cases.

Private fill freshness uses Number.isFinite's non-coercing numeric guard before
reading either clock. `private_fill_timestamp_types_match_the_maintained_handler`
drives actual account_all data.fills frames against maintained TypeScript arrow
bodies for 15 timestamp values, retaining the five-second boundary and future
numeric timestamps while rejecting strings, arrays and other non-number inputs.

Public REST payload parsers retain native arrays and row references. Book fallback
selection stays lazy, ticker candidate capture stays eager, and guarded book-side
reads occur twice as in the source. Native source tests retain UTF16 and sparse rows.

Private market-payload flattening retains native JS rows and skips array holes
without copying through JSON. The orders wrapper applies its object guard before
calling the live owner helper; source oracles cover identity, numeric bits and UTF16.

The TransactionClient JS decoder treats null trigger/expiry fields as absent and
maps unknown order-type/time-in-force values to the source limit/GTC defaults.
The real SDK/crypto regression compares 31 exact preimages and computed defaults.
Nonce retry classification reads the original error's code and message directly.
Both String conversions precede the final OR, so matching codes still observe
message coercion failures. Unread surrogate fields and cycles stay untouched.

The TransactionClient approval binding retains raw l1Signature metadata outside
its typed signing parameters. Falsy values remain visible to callers and are
omitted by the actual SDK request serializer, matching the maintained source.

Signer class flag inputs use JavaScript truthiness for preimage bits and preserve
original JS values in returned order metadata. Group validation shares one body
with typed callers and receives strict raw-side equality from the JS binding.

Cancellation and market-precision/minimum helper globals consume native JS fields,
without whole Order/Market decoding. Keep market lookup and precision fallback
lazy; unused properties and metadata must not be inspected or serialized.

The constructed native ticker census in `tests/exchange_binding.rs` drives
1,240 field-independent cases through 2,280 public socket frames. All numeric
aliases, nested book-side forms, ticker batches and market-stat batches receive
eight malformed/coercion values between distinct good rows. Native binary64
fields are checked against the constructed TS exchange and DefaultStore, with
finite resets before every case. The shared control runner misroutes mark/index
in the live ingress body and requires compiled failure and restored success.

Model `info` uses canonical `RetainedJsonInput` inside its existing presence
wrapper. Read numbers and UTF-16 fields directly before any JSON report; source
computed numbers use `Float`. Consumer coercion uses the shared retained Number,
parseFloat and String helpers. JSON rendering belongs at the explicit output
boundary, with no alternate model-info carrier.

Structural alias receipts use compiler-authenticated declarations and artifacts via
`scripts/structural-alias-contracts.mjs`. Six generic aliases have one exact
production-shaped argument each; they make no claim for other instantiations.
The selected result/promise/query payloads have no Deserialize implementation.
`ReadonlyWarningTimes` does implement Deserialize: its receipt distinguishes the
empty JSON map from native UTF16-sequence keys, checks duplicate overwrite and
binary64 bits, and observes mutation through the decoded shared cell without
claiming HashMap iteration order. Constructed parent tests exercise actual SDK
query/result identity and the private class-wide warning cooldown; signed order
placement exercises real promise continuation and native crypto. The publicly
exported SDK validator is exercised through its native function entry separately.
`scripts/structural-alias-controls.py` compiles independent production faults and
requires named failures followed by exact restoration.

`SOURCE_CORPUS_PIN_REVIEW.md` reviews the eighteen source-provenance guards
following unrelated type-only additions to the shared TypeScript types barrel.

The fourteen TransactionClient get/submit methods have a real-constructor timing
proof in `TRANSACTION_TIMING_AUDIT.md`. Keep live JS inputs until the nonce await
settles, retain the batch iterator, and preserve receiver dispatch for submits.
The typed FIFO promise family is already eager; do not wrap it in a second driver.

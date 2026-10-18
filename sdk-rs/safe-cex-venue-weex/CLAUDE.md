# WEEX Rust venue

`WeexExchange` and `WeexDependencies` expose the production composition. The
concrete binding supplies every class member through the existing native bodies,
shared js_runtime services, native-number bridges, Store facade, HTTP factory,
retry engine and BaseWebSocket lifecycle. The generic class interface retains
its separately declared host contract; older extraction-stage notes below refer
to those standalone components.

The host supplies HTTP/WS runtime, an optional external-data factory and local
wall-time conversion. The native registry supplies the common signed HTTP host,
gateway factory and UTC date profile, with its existing caller-owned limiter.
The source's inherited scheduleCancel rejection remains `Not implemented`.
Typed placement enters the shared ValidatedOrder/ValidatedBatch path before the
native body; batch capability registration invokes the source batch algorithm.
The trading runtime's `updateStoredOrder` capability binds the real Store's
`updateOrder` with its Store receiver. Accepted single/batch PostOnly placements
and TP/SL amendments then apply the source patch and return IDs.
Successful batch replies require a positive ASCII decimal order ID, exactly like
the source regex. Every result is validated before any PostOnly Store patch;
leading zeroes, signs, whitespace and nondecimal strings visibly refuse the batch. Constructed
regressions check existing and absent rows, batch-result reordering and both
protective order types; an absent row remains absent.

The explicit Weex bigint response parser consumes original wire tokens on both
success and HTTP failure before venue status/error handling. Private WS uses the
shared prepared dial operation with required headers and a retained auth provider
for every attempt. The inherited exchange dispose reaches source socket overrides
through the shared JsProtocol hook: public cleanup removes its symbol listener,
and private cleanup cancels all three refresh timers before core disposal.
Source-own methods retain weak receivers; externally returned unsubscribe closures
and pending snapshot observers retain their host until cleanup or settlement.

The source nukePositions body replaces Store.positions with the refetched snapshot.
It does not merge positions absent from that response. Constructed exchange tests
call the public Exchange trait and cover both nonempty and empty
REST responses, stale IDs, updated contracts and exact fake-zero snapshots. The
public trait wiring remains separate from this Store-policy correction.

Positions and historical fills use the source's default extractArray admission:
raw arrays and the `data` array only. Extra `items`, `list` and `rows` wrappers
are ignored. Constructed HTTP regressions cover all six direct/nested extra
wrappers plus the two accepted forms, retaining fake zero positions when needed.

Historical-fill pages apply the final strict-field policy per row. The concrete
normalizer warns with the offending field/value and refuses only an Execution
whose maker or commission asset cannot inhabit the shared model, retaining all
valid rows. Missing maker stays absent, false stays false, and assets are never
coerced. Source coercion/capture errors retain their order and propagation;
warning subscriber failures also propagate. Both the public utility and the
binding use one normalization pipeline; standalone utility error names/messages
remain unchanged. This intentionally differs from TS retaining malformed rows.
Mixed-page and all-refused-page regressions use the constructed exchange.

Market snapshots apply the same strict-field policy per row. Source coercions
finish before required-string admission; a malformed symbol or base is reported
with its field and received value, then only that row is omitted. An all-refused
page returns an empty snapshot. Source coercion/capture failures still use the
existing error/cached-market catch. The utility and binding share one normalizer;
the utility's prior error messages stay unchanged. Constructed tests cover twelve
mixed-page refusals, an all-refused page and source-error precedence. An actual
maintained-method probe separately pins TS retaining all raw fields; this is an
intentional typed-output difference, not arbitrary malformed-input parity.

`tests/exchange_binding.rs` drives production methods through scripted native HTTP
and WS hosts. `tests/native_host_weex.rs` in the registry verifies the actual
request HMAC over loopback HTTP. `scripts/binding-controls.py` independently
disables each of the sixteen required bindings, requires its compiled native
witness to fail and restores the complete target. Standalone algorithm/source
oracles and their existing mutation runners remain independent evidence.
Gap re-snapshot failures and routed-callback throws are silently discarded,
matching source. Initial book subscription failures retain their source warning.
Constructed HTTP/socket coverage checks no gap warning, no failed replacement
delivery and a later contiguous update against the retained book.

Remaining reporting and async ownership choices remain priced at
[weex-async-observation-choice](../parity/MANIFEST-SPEC.md#weex-async-observation-choice);
normalizer record boundaries keep their existing declarations.

`fetchOrders` retains only fulfilled normal/algo legs and silently discards each
rejection, matching Promise.allSettled in the maintained source. Its constructed
regression checks successful rows and the actual error emitter after one HTTP
leg rejects. The removed reporting capability and old native-only expectation
are no longer part of the account-read runtime; source order/dedup mutation
coverage replaces the former reporting-divergence control.


The frozen 35-row divergent audit is in `DIVERGENCE_AUDIT.md`. Class assembly and
async observation choices cite their explicit MANIFEST-SPEC anchors; ready
private-refresh reactions suspend before disposal checks and updates. All three
refresh callbacks silently catch fetch and Store-update failures, matching source.
The real private-account-frame regression rejects its balance REST refresh and
checks that the old balance survives with no host exception report.

The API request callback signs an already serialized string body verbatim on
replay, matching the bytes Axios sends. Other truthy values still use JSON
stringification. `tests/replay-signing-oracle.cjs` executes the source interceptor
for eight body cases, independently comparing signatures through the native
callback in `tests/runtime.rs`. Removing the native string guard compiles and
fails the serialized-body signature assertion.

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/weex/CLAUDE.md).

`src/types.rs` mirrors the 26 value constants in `weex.types.ts`. Mapping tables
use ordered key/value slices and the landed shared enums; endpoint lists and
interval sets preserve source insertion order. These declarations expose own
entries, not JavaScript object prototype lookup or mutation. Future normalizers
must account separately for inherited lookup behavior at their call sites.
`RECV_WINDOW` is binary64, matching the source Number.

`src/response_types.rs` represents the three response declarations. Kline tuple
slots remain positional, with binary64/string unions. Order-result fields retain
missing versus present (present null fails the declared shape). Exchange-info
records preserve arbitrary JSON via `JsNumberInput`; source `Record<string, any>`
also accepts arrays, so its Rust representation has object and array variants.
Capture before buffering at every public Deserialize boundary, and at whole entry
when callers wrap these types in tagged/untagged envelopes. Never stage through
`Value` before capture. Output eligibility uses explicit expression-macro impls.

Type aliases are erased by TypeScript: their witnesses use the live checker for
the declarations and execute the actual shared `jsonParse` for JSON values.
They do not claim Axios validates shapes. Typed records project declared fields;
extra top-level properties are ignored, with explicit extra-property fixtures.
This slice does not wire REST's bigint-string response transform.

Response-row rulings cite
[weex-record-shape-choice](../parity/MANIFEST-SPEC.md#weex-record-shape-choice),
[weex-fill-projection-choice](../parity/MANIFEST-SPEC.md#weex-fill-projection-choice)
and [§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
The response target checks declared shapes, native fields and depth-150 refusals;
record_boundaries adds differentiated native names/IDs and exact source unit
observations without treating the aliases as runtime validators.

`tests/accepted_responses.rs` separately inspects the four response owners through
raw and qualified Deserialize calls: optional fields, object/array variants and
binary64 bits precede any owner serialization. The accepted-set contracts execute
additional direct native projections. Run `tests/accepted-response-controls.py`
alone to remove each object response's guard in turn: a complete valid array in
Serde field order must fail the named assertion, then restoring the guard must
pass all four tests. The record union continues to admit arrays.

Forty-nine selected helper contracts record individually reviewed dispatch,
request-construction and host-operation roles with exact compiler signatures.
Their native receipts are classification evidence. Response owners use the
separate executed decoder observations above.

Retained-input, typed-error conversion, present-field, optional-boolean and
public-trade contracts execute direct native adapters. The three generic class
constructors have typed API classifications; their concrete Factory behavior is
covered by the production constructor E2E, including both source sockets.

The concrete Host and Factory contracts additionally execute native property
lookup, receiver binding, requests, callback bodies and prototype linkage. The
concrete Exchange contracts call all twelve selected raw-input methods through
the production binding with scripted HTTP/WS hosts. They inspect native model
fields and numeric bits, request effects, and order-book delivery and cleanup.
`accepted-set/weex-binding-fixture.rs` reuses the E2E I/O hosts for these adapters;
it replaces no venue method. The constant-table projection uses a module-scope
typed serializer helper instead of a local macro with opaque declaration effects.

Run `tests/accepted-binding-controls.mjs` alone to remove capture, undefined
omission and side-normalizer routing in turn. Each compiling production mutation
must fail its native contract; exact source restoration must pass.

Seven enum normalizers in `src/utils.rs` mirror their source control flow and use
the actual shared tables. `WeexUtilityInput` captures its opaque `value` with
`js_deserialize_passthrough` before constructing Value, with explicit passthrough
eligibility. Missing arguments are None; present null remains captured. The
retained JSON contract excludes executable coercion, symbols, BigInt, native
NaN and modified prototypes. Those choices, UTF-8 and bounded recursion are
declared CHOSEN per row. Source supplements demonstrate callable coercion
returning nondefault values and 150 nested arrays succeeding before Rust refuses.
Mirroring arbitrary dynamic objects or unbounded parsing per venue would bypass
the shared transport value and ingress contracts.

`to_number`, `to_integer_string` and `normalize_percentage` also follow their
source branches, reusing shared JS string coercion and parseFloat. Their native
number arguments retain NaN/Infinity and signed zero; only capture canonicalizes
NaN. The same retained-input divergences are declared per row.

The full venue surface is indexed by the reconciled manifests under
sdk-rs/parity/manifest/exchanges/weex/. Local source witnesses and live venue
execution are separate evidence scopes.

## Scoped verification

Set `SAFE_CEX_TYPESCRIPT_PATH` to the installed TypeScript compiler directory.
Missing compiler input fails the oracle rather than skipping it. From repo root:

- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 1`
- Host and wasm clippy separately, with `-p safe-cex-venue-weex --all-targets --jobs 1 -- -D warnings`
  (wasm adds `--target wasm32-unknown-unknown`).
- `python3.12 sdk-rs/safe-cex-venue-weex/tests/mutation-check.py` changes one
  production constant at a time, requires its named `constant::<name>` failure,
  restores source in `finally`, then requires the entire restored oracle to pass.
  Run it alone: it intentionally writes `src/types.rs` temporarily.
- `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py` parses manifests
  and exemptions with real tomllib and invokes the canonical parity binary with
  a WEEX-only workspace projection under ignored `sdk-rs/target/`.
- `node sdk-rs/safe-cex-venue-weex/tests/extractor-check.cjs` invokes the canonical
  `extractFile` implementation on six WEEX sources, checking identities/kinds;
  options/global diagnostics and source-local semantic diagnostics must pass.
  This is not a repository-wide TypeScript typecheck.
- `node sdk-rs/safe-cex-venue-weex/tests/layers-check.cjs` checks WEEX's actual
  workspace dependency closure against the shared layer policy.

`types-oracle.cjs` transpiles and executes the actual WEEX module, shared types,
and logging enum source. It captures Number typeof/bits before serialization,
compares every own entry and export, and consumes no shared fixture corpus.

Response witnesses use one Node subprocess for the entire corpus. Native tuple
slots, struct fields, record variants and f64 bits are compared before any Rust
wire serialization. All capture headers record CPU architecture, full Node
version and Node major; only NaN is canonicalized, never signed zero. The response
capture tags arrays and objects separately. Positive supplements differentiate
all same-typed order fields and kline slots, assets versus symbols, and nested
empty/nonempty object versus array shapes. The census uses checker-resolved
properties and includes a permanent inherited-required-field control.

During porting run only the affected `--test` target with `--jobs 1`. At merge-up
readiness run the full scoped suite and separate host/wasm clippy sequentially. Run
`tests/response-mutations.py` alone for response/census/capture controls; it
restores all mutated files in `finally`. Symmetric serde swaps must fail native
field assertions even though re-encoded bytes would match. Re-run the earlier
26-constant controls at slice boundaries.

`tests/utils_oracle.rs` executes seven verbatim TypeScript normalizer declarations
in one Node invocation, with actual source enums/tables. It compares native enum
variants and thrown name/message across direct raw, numeric raw, owned Value and
borrowed Value routes. Numeric supplements inspect retained native Number bits
separately because enum defaults collapse numeric distinctions. The 228-case
corpus covers every table key, lowercasing, whitespace, nested arrays, stock
inherited names, Unicode case folding and coercion exceptions. No JSON re-encoding
serves as the result assertion. `tests/utils-mutations.py` changes one production
lookup at a time and requires its named assertion failure before restoration.

`tests/numeric_utils.rs` runs 533 cases through four routes with one Node process.
It compares native output strings or f64 bits. Actual source numeric `-0` becomes
positive zero through String, so the string `"-0"` supplement differentiates it.
`tests/numeric-utils-mutations.py` proves three production function mutations.

Timestamp helpers use shared Number coercion and V8 Date.parse. The required
`local_to_utc` callback represents the source host timezone, including DST
ambiguities; there is no default UTC assumption or venue-local timezone database.
`tests/timestamps.rs` runs the complete source corpus once in Node, comparing
native bits/errors across four routes in UTC and two fixed-offset zones. It
covers all unit thresholds and nearby representable floats, local and zoned dates, pre-epoch
flooring, overflow, coercion errors, and retained-input refusal supplements.
`tests/timestamp-mutations.py` proves unit scaling, negative floor and timezone
adapter controls. This slice remains local while origin is pinned at 181d8abcac.

For the 9419b340d8a6 Store ABI cutover: source-module and dependency-closure
inventory finds no Store capability or Store call site in this crate. Implemented
modules are constants, response codecs and pure utilities; exchange and socket
rows remain pending. No mapped row was FINDING against the notification contract.
At fetched integration 9af82bfeaefa3496cd8eea0a29b9ce7d3c1741a7,
ExchangeErrorMetadata now carries store_subscriber_ids. Both recovered and
fallback notification conversions append notification.subscriber_id(), retaining
inner-to-outer ordering. IDs are Store-local, not globally unique. The earlier
9419b340d8a6 projection gap is closed by this shared contract; WEEX had no mapped
FINDING to promote and no original Store error box to simplify. Reaudit actual
propagation when Store consumers are ported.

Order-book helpers retain source input order, string iteration, numeric filtering,
nullish alias precedence and coercion error order. They call the shared
`safe_cex_store::orderbook::calc_order_book_total`; this adds a Store-crate
library dependency for a pure helper, without instantiating Store or notifying
subscribers. WeexUtilityError preserves name/message (V8 diagnostic wording).
`tests/orderbook.rs` executes the actual venue declarations and shared TS total
helper with one Node process per corpus; assertions read native level fields and
bits before encoding. Multi-level, distinct-side fixtures break the source's
first-level amount==total invariant; production field/side/amount-total swaps
must fail in `tests/book-mutations.py`. No source/shared fixture is regenerated.

Earlier-row swap audit covered all 26 constants: eight rows contain 12 fixed
source alias groups (26 equal-value pairs); INTERVAL has 12 key=value identity
pairs. These input-free values cannot be distinguished by a new input. Whole-entry
swaps remain observable by ordered keys. Fifteen enum-bearing tables now compare
native variants using explicit TS-label-to-variant matches independent of shared
Serde before the existing encoded-shape checks. The original 26 controls exercise
keys/members; `tests/native-constant-mutations.py` additionally swaps distinct
production variants in all 15 tables and requires `constant::<name>::native`.

`fake_positions_for_market` uses the landed shared ID helper and Position model.
Its witness executes original TS and createPositionId, checking native fields,
84 parameter combinations, and three production controls. Fixed zero fields
cannot be differentiated by input; controls perturb a constant instead. Hedged
mode differentiates id/symbol, while long/short and nonzero leverage distinguish
side/order and carried numeric values. Market info is intentionally not copied.

Income reads live in src/income_reads.rs and retain their runtime capability
interfaces. Shared income paging remains owned by its shared implementation;
WEEX adds no venue-local pager. The maintained TypeScript audit-regression feeder
stays unchanged. The normalizeIncome utility ruling and its transaction-ID
witness do not establish REST assembly or pagination behavior.

The fills slice implements `fills::normalize_execution` and
`normalize_historical_trade`. Both reuse shared models and existing retained
coercion/timestamp helpers, with no Store, network or income paging operation.
Execution follows alias/number evaluation before its validity guard, truthiness
of nullish-selected fill IDs, default-zero PnL, side table defaults and commission
NaN/zero omission. Historical trade shares the execution scalar stage, guards
its order ID, then adds quote quantity and the second timestamp conversion.
Both public functions admit their closed output fields after source work.
Its local-to-UTC callback is reusable because zone-less dates are parsed twice.

Input/model costs are declared once at
[weex-fill-projection-choice](../parity/MANIFEST-SPEC.md#weex-fill-projection-choice);
returned-unit observations use [§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
The supplementary fills_error_order target pins guard, coercion and timestamp
order alongside whole-input refusals. Original malformed-field fixtures remain
unchanged. The finite native inventory stops at 27 cases over four ingress routes
for both functions: 216 outcomes, 120 successful records, 52 null results,
44 errors and 548 numeric comparisons. Strings, enums, optional presence and bits
are inspected directly. The oracle executes actual omitUndefined with lodash.

`retry::clamp_weex_rate_limit_delay` delegates shared math extrema on native f64.
`parse_rate_limit_interval_ms` accepts the source string as UTF16 code units,
including lone surrogates in prefixes, and matches the anchored ASCII digit/unit
suffix. It preserves finite interval checks and multiplication overflow. These
two rows are ported; no dynamic retained-value or UTF8 narrowing is required.
Their actual-source oracle stops at 12 clamp cases and 23 interval cases.

Run only `--test fills --test retry` for this slice with the configured TypeScript
compiler. tests/fills-retry-mutations.py runs four serial compiling controls, one
per newly mapped function, restores every source file in finally and requires
both affected targets to pass. Retry transport configuration remains pending.

Follow-on candle utility (local while PR 7272 is pinned at 30a87ef472):
`normalize_candle` follows tuple projection, nullish object aliases, timestamp
normalization and numeric coercion in source evaluation order. It retains the
source finite-value guard; within retained JSON, the called helpers already
produce finite values, so no false claim of a distinguishing guard fixture is
made. Tests run actual source once in Node for 23 cases across four ingress
routes. Six distinct tuple fields and six distinct object fields each have an
independent production mapping control requiring compilation and exit101 at the
native field assertion. The original identical-error case cannot establish evaluation
order: a separate source Date.parse trace and Rust timezone-callback assertion
distinguish a zone-less timestamp from a later throwing open. A thirteenth
production control reorders those initializers and must fail that assertion.
Source callable, UTF16 and depth supplements record the
retained-input boundary. No shared fixture is changed.

Precision audit is per output field. At 30a87ef472 no implemented Rust precision
read or step adjustment exists: zero fields were swapped, a census rather than a
coverage verdict. Pending updateOrder has one price adjustment; formatOrderPayload
has six price adjustments and one amount adjustment. Before implementing them,
preserve any existing corpus byte-for-byte and supplement fractional distinct
steps across several step pairs, both sides and all supported order types. Test
native steps and each payload field separately (including fields sharing an
adjustment result), prove each production swap compiles, fails its named assertion
with exit101, and restores. A passing control for one field covers no other field.

Cardinality audit (follow-on to 46a814049e): the original fake-position target
accepted both an empty and a one-row expected positions list, despite producing
two native positions. These were expected-side false greens, not production
empty-output survivors. Thirteen production empty/subset controls across the
four collection-returning targets failed the original assertions. Scalar-returning
targets have no native collection output to empty; their corpus walks still need
independent admission checks.

All eight targets now use `tests/support/cardinality.rs` with the fixed
`cardinality-census.json` inventory. Seven behavioral corpora check count, unique
input identities and ordered input-identity FNV64 (a regression checksum, not a
security hash); constants require 26 source exports. Supplement collection counts
are also asserted. Book source counts are independently pinned per successful
case and to 40 total levels; native lengths are compared before every zip, and
160 level comparisons must execute across four ingress routes. Fake positions
require 84 cases, 168 source outputs, two expected outputs per case, a separate
native/expected length equality, and 168 executed position comparisons. The old
native two-position assertion remains. Response ingress route counts are pinned
at six, or three for declared UTF16 refusals. Explicit main comparison totals are
26 constant exports, 273 response routes, 6384 enum outcomes, 2132 numeric outcomes,
1176 timestamp outcomes and 92 candle outcomes. Counts are assertions, not printed
evidence. These are outcome comparisons, not a claim of every nested field count.

`cardinality-mutations.py` covers empty, subset and extra native outputs, repeats
those controls with source census checks bypassed, and empties both book sides
with source checks bypassed to require the explicit comparison count; run serially.
`census-mutations.py` drops or duplicates main expected rows, drops expected
positions, then bypasses the two source-position count checks to prove the
independent native length check still fails. Both scripts require compiled
exit101 named equality failures, restore in finally, and require restored targets
to pass. Eight original oracle files and response-cases.json remain byte-identical;
oracle SHA256 values are retained in the census file. No existing corpus inputs
were repaired or shared feeders regenerated. These checks do not claim protection
against arbitrary coordinated corruption of both the inventory and assertions.

Additional zero/zero controls: constant collections have independent per-table
source and native entry inventories. Response captures retain a fixed per-case
collection shape (every array boundary and object key), checked independently
against decoded native output. `paired-empty-mutations.py` empties both expected
and native collections while bypassing source checks; native entry/shape guards
must still fail. These supplements change no original fixture values.

`consuming-array-mutations.py` temporarily mutates the shared Array iterator in
this worktree to drain all retained input elements before returning None. It must
fail the response probe's native length assertion; with both lengths removed it
must fail the explicit comparison count. Only removing all three guards makes
that injected mutant pass, a labelled guard-removal control rather than evidence
for production. Five retained-input utility targets are also checked under the
same consuming mutation. The script restores the shared file byte-for-byte in
finally and reruns restored targets. No shared decoder change is committed.
Response tests share one Node capture via OnceLock even when both tests execute.

Review refinements pin identities for secondary arrays too (numeric probes,
refusals, depth and callable supplements), not just their lengths. Duplicate
replacement controls cover all four enum supplement groups. Constant inventories
retain every nested collection boundary, including both BASE_WS_URL endpoint
lists; the paired-inner-empty control preserves the outer two rows while proving
the independent native shape assertion rejects both inner lists being lost.

Candle follow-on based on integration dbdf3d15dd: the three response types
already landed as CHOSEN divergent rows; they are not pending work. The candle
witness pins 23 ordered case identities, two refusal cases, and 92 executed
route comparisons. Source empty/subset/extra/duplicate controls and a refusal
subcorpus removal must fail their named census assertions. The original candle
oracle remains byte-identical to its previously reviewed capture source.

A review control replacing the UTF16 refusal with the depth refusal passed the
initial count-only candle target. Both ordered refusal raw inputs are now pinned;
the same-length duplication must fail candle::refusal_identity. No oracle inputs
were changed in the repair.

normalize_balance follows source USDT find order, asset-or-coin truthiness, first-row
fallback, numeric evaluation order and frozen-or-Math.max used balance. It calls
shared coercion and math_max, retains the complete input array in native info,
and does not create a venue-local numeric or notification adapter. The 18-case
actual-source corpus runs once in Node and compares four native numeric fields
plus retained info across four ingress routes (72 outcomes, 192 field checks,
48 info checks). Case and refusal identities and source/native lengths are pinned.

The balance review injected native info objects as entries arrays; the initial
untagged capture passed. Direct native retained-value equality now independently
checks object/array identity against captured input, in addition to source bits
and per-case lengths. A permanent compiling shape control requires
balance::distinct::route_0::retained_info_identity.

public_trade::normalize_public_trade follows source number/timestamp evaluation
before the validity guard, nullish id fallback and strict buyer/m/isBuyerMaker
booleans. Explicit undefined in toNumber uses default zero, so quote_qty is
Some(0) when absent. Timestamp input zero goes through source Date.parse, rather
than necessarily producing zero. The 79-case corpus has 316 route comparisons,
288 successful records, 12 null results and 16 errors; native bits/strings/enums
are checked directly. pick_retained is crate-private and projects already captured
values only, without exposing an unchecked public ingress constructor.

The public-trade review duplicated one flag triple across all 64 flag IDs; the
initial target passed. Each decoded triple now matches an independent Cartesian
matrix. Balance and public-trade witnesses also pin ordered id/raw input FNV64
checksums (regression checksums, not security hashes), preventing relabelled input
duplication. input-census-mutations.py recomputes actual TS results after input
duplication and requires input_identity or flag_inputs failures. Oracle files
remain byte-identical; these guards do not claim immunity to coordinated edits
of both the test inventory and the oracle.

income::normalize_income is record normalization only. It reuses shared category
classification and source timestamp/numeric/string helpers in source evaluation
order. The 34-case actual-source corpus covers all 16 named category spellings
plus unknown, nullish/truthy identifiers, numeric ingress and coercion failures;
136 outcomes, 108 successes, 28 errors and 216 numeric comparisons are fixed.
Native trade-id presence and source field counts are independently pinned by
case, including truthy empty arrays yielding Some(empty string). Raw input
identity and UTF16/depth refusal identities are pinned. Both REST income rows
remain pending; no pager implementation has been added here.

`market_data::normalize_market` and `normalize_ticker` project into the shared
Market/Ticker models and make no Store calls. Market precision delegates to
shared parse_int and parse_precision, adapting CCXT's radix inference and its
negative/NaN count behavior (0.1) before string materialization. The nested
quantity fallback is evaluated even when minOrderSize is present. Market record
choices are declared at
[weex-record-shape-choice](../parity/MANIFEST-SPEC.md#weex-record-shape-choice).
Limit/fee coercions finish before required-string admission; record_boundaries
pairs fifteen malformed-field/coercion inputs with actual source errors.
The market_data target also retains its three malformed-record witnesses.
Ticker retains nullish alias precedence, book quote priority, numerical defaults,
percentage branches, and own Undefined versus present-null info entries. Its typed
Market argument carries id and symbol unchanged.

`tests/market_data.rs` executes actual source plus the imported CCXT helper for
12 market cases and 14 ticker cases, comparing four ingress routes, 80 successful
records, 24 errors and 816 native numeric fields. It asserts retained info
without re-encoding model outputs. The finite inventory stops at these cases;
it is not a claim of exhaustive dynamic-object coverage. Callable source supplements retain their separate executable-value boundary.
Unpaired UTF-16 data is admitted and retained before typed String projection. Run the affected
`--test market_data` target with the configured TypeScript compiler. The two
serial controls in `tests/market-data-mutations.py` swap market price/amount and
ticker volume/quoteVolume separately, require their named native assertions,
restore source and require the target to pass.

Precision counts above the shared 536,870,888 string-length ceiling are refused
before converting the parsed Number back to text. This prevents exponent notation
from being re-parsed as a small prefix. A native test covers exponent-sized and
positive-overflow string counts in both precision fields. It deliberately does
not execute CCXT's enormous concatenation loop on the shared host; the source
failure beyond this limit is a control-flow inference, not a measured oracle exit.

For recovery verification, tests/recovery-family-mutations.py runs one compiling
production control for each of the eight recovered row families, plus merged
census/refusal controls. It restores and reruns the affected target after each
control. Results belong in the tracked CANDLES_GATE_EVIDENCE.md at the measured
code SHA; recorded oracle hashes are audit metadata, not runtime enforcement.

The next retry-policy slice implements parse_retry_after_ms,
is_weex_read_only_request and should_retry_weex_request in src/retry.rs.
WeexRetryAfterValue models the source typeof partition without refusing inputs:
Number is native f64, String is UTF16, and all immediately rejected kinds are Other.
The caller supplies the source clock and Date.parse capability, with original
UTF16 arguments and source call order. The local request decisions use declared
Axios field projections; the actual imported network classifier is an explicit
host callback, invoked only after read-only admission. These three rows are
ported local decisions, not a claim to have ported the host Date/clock, classifier,
arbitrary Axios object/getter runtime or transport retry configuration.

The finite actual-source test in tests/retry_policy.rs stops at 29 Retry-After,
19 read-only and 19 policy cases (67 native results), with 17 date-parser calls
and 16 network-predicate calls. It compares native bits/booleans, original date
arguments and independent call-count expectations, including nonfinite values,
UTF16 surrogates, Unicode method spellings, rejected kinds, cancellation/TLS
exclusions and the rate retry budget. The oracle loads the installed axios-retry
classifier. tests/retry-policy-mutations.py proves one compiling named failure
per newly mapped decision, restores source and requires the affected target pass.

`tests/record_boundaries.rs` checks native strings, whole-input refusal and market
error order against actual source profiles. `utils::owned_snapshots` mutates
private captured fields and returned info in both directions under `cfg(test)`;
it adds no public mutable ingress. Its source controls use the frozen audit
observers, while `record-boundary-oracle.cjs` selects current utility declarations.
Snapshot rulings cite [§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias);
Market/response admission rulings cite the record-shape declaration above.
Native assertions precede oracle-wire comparisons, and existing corpora remain
unchanged.

The typed Exchange `fetch_leverage_for_symbol` entrypoint uses the existing WEEX
symbol-config member. Cross leverage and separate isolated buy/sell values update
`fetchedLeverage`; the market maximum belongs only to `fetch_symbol_max_leverage`.
The production binding tests distinguish those paths through the public trait.

WEEX's assembled UUID callbacks and client-ID mint remove both nanoid separators
before prefixing or truncation, matching the source UUID helper. The production
binding regressions retain caller IDs verbatim and exercise generated ordinary,
conditional, protective and minted IDs with controlled runtime entropy. Real
private frames additionally cover order/position Store writes, the REST balance
refresh triggered by account frames, and the emitted fill event.

API signing and private WebSocket upgrade headers read the shared synced
`VIRTUAL_CLOCK`, matching the source's `getCurrentTimeMs()` calls. Retry-After
parsing and public socket pacing retain their source raw clocks. The production
binding's `synced_clock` regression independently checks HMACs and synced time
for REST and two private dials with different server corrections, so reopening
must reevaluate the current offset.

The typed Exchange `nuke` and `nuke_positions` entrypoints dispatch to the existing
WEEX members: close all venue positions, replace the refreshed snapshot, then
(for `nuke`) cancel ordinary and algo orders. The source has no retry-count input.
`exchange_binding` proves endpoint reachability from an empty Store and close/refresh/
cancel order; private order, position and account frames assert actual Store changes
alongside the retained fill-event witness.


`exchange_binding::production_ticker_coercion_matches_constructed_typescript` adds
96 field-independent cases through the constructed production socket/Store:
consecutive ticker frames, object/array payloads and finite fallback. Each case resets both native and
maintained TS Stores to finite baselines, then checks all eleven native numeric
fields after malformed and good rows. Null, numeric/prefix/NaN strings, objects,
overflow, signed zero and false follow the actual TS exchange and DefaultStore.
The shared ticker census records the witness. The compiling
`parity/scripts/ticker-native-controls.py weex` fault must fail that test and
pass after exact source restoration.

Utility inputs directly own canonical RetainedJsonInput. Ticker info always creates
both ticker and bookTicker keys, with absent arguments as Undefined. The constructed
REST regression checks raw UTF-16 units, nonfinite bits and own property presence.

Retained utility inputs admit lone UTF-16 surrogates. Enum and numeric witnesses
inspect the native units before invoking the actual normalizers; numeric results
use binary64 bits. Closed String outputs still visibly refuse lone units.
Ignored surrogate fields reach the fill guard/mapper without narrowing the whole
record; depth-budget refusal remains separate. The existing source corpus and
all ordinary ingress-route comparison counts are unchanged.

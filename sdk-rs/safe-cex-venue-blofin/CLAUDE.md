# BloFin Rust venue

The public connection-failure notice emits two listener arguments: the notice
text and `{forceToast: true}`. `tests/native_warning.rs` drives the real native
venue and its twenty-second timer, checking the argument shape and one-shot notice.

The maintained behavioral source is
[`blofin/CLAUDE.md`](../../packages/safe-cex/src/exchanges/blofin/CLAUDE.md)
and its seven TypeScript files. `src/types.rs` is the first slice: constants,
lookup-table own entries, and six wire response declarations. No complete exchange
or websocket implementation is registered yet. REST helper slices consume
configured shared adapters.

Response decoding enters through `safe_cex_num::numeric_ingress` and explicit
`JsNumberOutput` registrations. All response scalar fields are strings in the
source, including decimal amounts and timestamps; numeric conversion belongs at
the consuming method. Required fields stay required; optional fields distinguish
omission from present null (null refuses). Records use shared map-only decoding.
The batch leverage side is a closed net/long/short enum. Unknown properties
are omitted; shared ingress enforces its recursion budget before projection.
Its decoder admits strings before delegating to a private remote enum companion,
so ordinary Serde and qualified decoding also refuse tagged records such as
`{"long": null}`. Keep the three rename attributes and serialization unchanged;
`batch_position_side_accepts_only_declared_strings` covers the ingress paths.
Duplicate JSON keys follow the shared capture’s last-value semantics. No opaque passthrough
fields exist in this slice. Future open-record readers must capture opaque
fields before constructing Value. Constant tables hold static JSON data only;
future consumers must use shared `js_table_lookup` for prototype-sensitive reads.

`tests/types_oracle.cjs` executes the entire maintained constants module and
its real imported enum modules with the compiler at `SAFE_CEX_TYPESCRIPT_PATH`.
Type aliases erase: their witness is an AST schema census plus JSON.parse in a
TypeScript typed identity, not a claim of runtime validation in the source.
The census refuses unsupported syntax and inherited interfaces. Rust tests check
native decoded fields individually before also checking wire output. Values are
different for every string field. `types-schema.json` is a reviewed schema pin;
it must not be silently refreshed when a source declaration changes.

Scope gates to `-p safe-cex-venue-blofin --jobs 1`: all-target tests, strict host
clippy, strict wasm clippy separately; fmt uses the same package scope. Tests
require `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

`tests/gates.py` runs the scoped gates sequentially. `tests/parity_scope.py`
uses the canonical extractor on the changed types file with source-file
diagnostics, then the canonical Rust indexer on this crate and all seven BloFin
manifests. This is not a whole-project extraction/diagnostic claim.

`tests/row_controls.py` requires one production mutation for every mapped types
row, using real tomllib for its coverage census. Each mutation must fail its
named runtime assertion at exit 101, then restore and pass at exit 0. It runs
only `--test types --locked --jobs 1`, sequentially, with one Node corpus per
test invocation. The five original false-green controls remain in
`tests/controls.py`; both runners reject compiler failures and exit 126 as proof.

`src/income.rs` ports the three `blofin.income.ts` helpers. `IncomeInput.value`
is an explicit retained passthrough captured by `js_deserialize_passthrough`;
enter through shared numeric ingress before invoking the mapper. These methods
return shared `IncomeRecord` and `ClosedPnlEntry` models, preserving native
numbers, including overflow from notional multiplication. The adapter supports
ordinary JSON values plus typed, non-mutating resolver callbacks; executable
objects and custom prototypes are outside its declared domain. No REST method
calls these helpers yet.

`tests/income_oracle.cjs` executes the whole maintained source and genuine enum
modules. Its 548 cases cover numeric coercion, mapper guards, differentiated
fields, fee directions, fallbacks, thrown error names/messages, callback traces,
and native binary64 values before JSON. Rust checks each native output field
on raw, owned and borrowed ingress. Three additional probes witness source
acceptance and Rust refusal of lone-surrogate/deep inputs. `income_controls.py`
proves one production mutation per income row with runtime exit 101 and a named
assertion, then restored exit 0. Each test uses one Node process for the corpus.
The coverage gate now extracts both mapped TS files in one Node process.

`src/order_history.rs` ports the three `blofin.order-history.ts` helpers.
`HistoryInput.value` captures retained JSON through the same shared ingress and
explicit passthrough registration. Optional numeric zero/nonfinite values remain
absent. The mapper constructs shared `HistoricalOrder` native fields directly.
Lookup uses shared `js_table_lookup`: inherited `constructor`/`__proto__` values
produce a declared `RepresentationError`, since the shared enum cannot carry
functions or prototype objects. The source identities are observed before JSON;
no fallback is silently substituted. `history_controls.py` proves three row
mutations plus an inherited-to-fallback mutation; each test invokes Node once.
`history_oracle.cjs` executes the complete maintained module and imported enums,
measuring 597 cases (36 numeric, 36 optional, 525 mapper), including four inherited
outputs and three additional UTF-8/depth boundary probes. Native fields,
undefined optionals, errors and resolver traces are compared across three ingress
paths. No REST consumer is implemented yet.

`src/utility.rs` ports eight stateless exchange helpers before their consumers.
Its oracle executes the maintained AST initializer text, original constants, and
actual imported enums; it does not instantiate `BlofinExchange`. Selection is
fail-closed for missing helpers, method shape changes and instance access.
The canonical coverage gate checks the full exchange source declaration census
and source-file diagnostics, alongside the other three mapped files.
`UtilityInput.value` has the same explicit retained passthrough contract.
Numeric parsing delegates to shared Number conversion, truthiness and String
coercion. The cancel-message regular expression is an ASCII literal with /i
and no /u; ASCII-only folding preserves its behavior for long-s and other
non-ASCII characters. The utility corpus contains 1,268 cases and three boundary
probes. All eight row mutations must fail their named native-value assertion
at 101, then pass at 0 after restoring production bytes.

Every behavioral oracle includes architecture, Node major and full Node version
in its capture header. Rust verifies the architecture and logs the header; full
scoped gates use `--nocapture` so the evidence retains it. NaN alone is canonicalized;
signed zero remains bit-exact. Earlier accepted SHA evidence is not relabeled
when capture metadata or later slices change.

`src/symbols.rs` ports the two symbol/id getters against the shared `Store`
contract. Stock inherited table hits remain JavaScript undefined (`None`), not
fallback strings. The oracle runs actual getter initializer text against actual
TypeScript `DefaultStore`; Rust uses production `DefaultStore` with the trait
update operation. Its 276 cases cover six market states, collisions, own keys
that shadow inherited keys, stock inherited names, Unicode and missing keys.
Two explicit replaced-prototype probes witness the shared typed Store boundary:
a `__proto__` market changes the TS dictionary prototype, whereas Rust keeps
own entries with its stock-prototype lookup profile. Lone surrogates remain
outside shared string ingress. No private store replica is introduced.

`src/tickers.rs` ports formatTickers using shared numeric coercion, roundUSD,
Ticker and Store. The actual initializer, actual roundUSD and actual DefaultStore
execute once per 220-case corpus. Tests compare native fields, retained info bits,
error traces, post-emitter fallback state and unchanged generic emitter identity.
Twelve inherited-market probes witness the declared typed-identifier refusal;
two UTF-8/depth probes witness ingress boundaries. Fallback is an owned snapshot,
not the source Store-array alias, with a named boundary observation. Three
production controls cover projection, emitter propagation and retained capture.
Oracle external dependencies resolve beside SAFE_CEX_TYPESCRIPT_PATH, matching
the isolated CI compiler/dependency installation rather than assuming root modules.

`src/orders.rs` ports mapOrders with shared multiply/subtract, existing venue
lookup constants and the previously ported non-swap helper. OrdersInput retains
open JSON before projection. The oracle executes the actual initializer and
imported source helpers, numeric functions, enums and DefaultStore in one VM.
Its 1,543 cases compare native order fields, info bits, error messages and clock
traces across three ingress paths. Opening triggers, authoritative TP/SL algo
records and normal orders retain their source branch order and reduce-only rules.
Invalid required enums/ids are an explicit RepresentationError boundary, before
that output clock call; the test observes the permitted valid-output prefix.
Source info aliases input and sibling outputs; Rust returns independently owned
values, witnessed before JSON. The clock callback is infallible/non-mutating.
Four controls cover amount math, refusal clock effects, invalid-side refusal and
retained numeric capture. Existing utility behavior is unchanged; its new crate
constructor only forwards an already-retained row into the source filter helper.

`src/positions.rs` ports mapPositions, including warning-before-mapping, retained
leverage/liquidation fallbacks and source position-side precedence. It reuses
shared create_position_id, numeric functions, Position/Store and venue constants.
The actual source initializer and imported dependencies execute once per 806-case
corpus. Cases compare all native fields, info bits, warning strings and unchanged
emitter errors on three ingress paths, including reentrant Store updates.
Nested source strict equality compares binary64 numbers and object identity,
not JSON structure. Source inherited sides or undefined inherited fetched leverage
have an explicit typed refusal. Source input-info aliases are observed; mutating
Rust output info leaves retained input unchanged. The six controls cover amount
math, numeric and reference equality, original-first-row warning text, emitter
identity and capture eligibility. Journal grading remains outside the venue;
these helpers reproduce source projections without assigning journal grades.

`src/inline_tpsl.rs` ports addInlineTpSlFallbackOrders before its REST reader
consumer. Shared typed Order ingress retains info, refuses malformed field types
and projects unknown Order/OrderExtra fields. Each of those boundaries has an
actual-source witness. The helper clones original orders and info instead of
exposing source object aliases. Its two passes seed deduplication from protective
records, then append TP before SL for eligible normal limit orders. Shared js_trim,
truthiness, coercion and parseFloat preserve the source filter and key semantics.
The corpus runs the actual initializer with actual enums: 266 cases across three
ingress paths, six refusal probes and one successful projection probe. Separate
native assertions cover TP price, SL price, remaining amount and native quantity.
Five production controls cover both price fields, native quantity, deduplication
and ECMAScript trim. This helper reads zero market precision axes; parsed TP/SL
prices do not establish coverage for future placement precision formatters.

`src/reads.rs` ports fetchTickersRaw and fetchTickers against shared RestAdapter.
Adapter construction/signing/retry remains the pending createAPI row. Raw data
retains missing versus null, ordinary JSON values and numeric lexemes through
TickerInput. Encoded JSON parses once into canonical UTF-16/binary64 Input;
opaque lone-surrogate text/keys and deeply nested arrays retain source admission.
Malformed encoded JSON remains an explicit ingress refusal. The separate lexical
typed decoder retains its depth guard. The generic emitter error remains unchanged inside
ReadError::Emitter. Native Untransformed/Transformed adapter values remain Input directly; strings
are never reparsed to infer provenance.
Arbitrary truthy JSON error messages are not string-coerced.
Reads return owned snapshots; the oracle observes the source response alias.

`tests/reads.rs` executes one Node corpus of 156 cases (26 frozen inputs,
three emitter modes, two read methods) across three response variants = 468 native
comparisons, plus nine phase checks and four ingress boundary probes. It
asserts source totals/per-case counts, independent native lengths, whole native
field/number comparisons, fixed input identity, exact request shape and executed
case-index coverage. `support/reads-inputs.json` is a reviewed input pin; do not
regenerate it to clear a failure. `reads_controls.py` runs scoped sequential
production, source, consuming-decoder and guard-bypass controls. Shared decoder
mutations are temporary and byte-restored, never committed. No formatter
price/amount precision site is introduced by these two wrappers.

position_adapter constructs shared canonical PositionRecord from position history.
This endpoint reports base units; contract_size is optional evidence-driven input,
never automatically the market's contract size. Refuse missing/net direction;
negate the venue fee while retaining rebates, leave funding unattributed, and use
historyId for round-trip identity. Native callbacks are infallible and row input is
retained JSON/UTF-8; dynamic executable/getter inputs are excluded. The focused
source test is --test position_adapter, enabled in ordinary Cargo runs.
It needs Node and SAFE_CEX_TYPESCRIPT_PATH; no live account or network is used.
No fetch wiring is claimed.

`src/modes.rs` ports four instance utilities ahead of their REST consumers:
getPositionSide, applyConfirmedPositionMode, applyConfirmedMarginType and
afterMethodCalledInWorker. changeMarginType applies the response's confirmed
marginMode, falling back to the argument, before refetching.
AccountModes is an explicit native cache context, not a replacement exchange
class. Mode reads use an asynchronous callback once with the selected symbol;
confirmed-mode updates use the real shared Store, set cache before callbacks,
release read guards before update, and keep the first position after ID collapse.
Worker result/argument inputs use existing retained UtilityInput; own JSON fields
preserve truthiness, while truthy unsupported mode values refuse at the closed
enum boundary. The source oracle witnesses that refusal boundary explicitly.
Owned typed Store snapshots exclude executable fields and JS object aliases.
The three native tests execute the actual TS method AST initializers with genuine
DefaultStore/enums. Their independent floors are48 side,6 update,12 worker cases;
one restored production mutation per newly mapped row is in modes_controls.py.
The full class, mode-fetch REST methods, and websockets remain pending.

`src/order_formatters.rs` ports formatTriggerOrder, formatNormalOrder and
formatAlgoOrder. FormatHost supplies actual async mode/margin, UUID, caller-ID
policy and cancellation capabilities; their source invocation order is kept.
OrderPayload is an own-property native map, inspected before transport records.
Lot sizes stay native contracts; numeric Number parsing for triggers differs from
parseFloat for normal/protective orders. Price precision, raw versus rounded
TP/SL, source trigger replacement identity and zero/full-position sentinels are
separate behaviors. Native split allocation is bounded to10000 payloads, with a
source10001-admission witness. Payload attachments and selected opposite orders
are owned snapshots rather than live shared JS aliases.

`src/order_services.rs` ports the mode/margin cache readers, leverage lookup,
confirmed mode POST, trigger placement and symbol cancellation. It consumes the
shared configured RestAdapter; API auth/timeout/retry construction stays pending.
Encoded bodies use JSON-only ingress; native/transformed values retain phase.
Leverage catch text for host errors uses Display rather than arbitrary JS-object
coercion. Symbol cancellation filters/delegates; cancelOrders saga stays pending.
The source oracle executes the exact method ASTs and genuine imports/DefaultStore.
Its floors are29 formatter and34 service cases, with service data on three adapter
paths. Named controls mutate one production behavior per newly mapped row.

`src/order_rest.rs` ports eleven placement/cancellation/update/leverage/margin
methods. WriteHost supplies typed, owned formatters, caller-ID guards, mode
readers, position refetch, placement and scheduler capabilities; wire writes use
the real shared RestAdapter. This is method orchestration, not a registered
exchange class. UtilityInput::json exposes retained callback provenance by
borrow, without re-encoding it. Native normal IDs stay strings; protective IDs
retain undefined/null/numeric source results using Option<Value>.
Normal placement/cancellation chunks are20; protective cancellation chunks10;
protective placement and trigger cancellation are sequential. Cancel batches
reconcile already-gone rows before surfacing real failures. Protective root-ID
comparison is exact, and trigger cancellation validates the whole input before
writing. Updates prefer remainingNative, keep edited TP/SL axes and require
provenance before replacing an opening trigger. Opposite zero-size protective
legs are recreated after the injected500ms delay. Schedulers also receive1000ms
for protective cancel batches and350ms for trigger cancels; actual timer behavior
is the host's responsibility.

Leverage reads both account modes, starts both hedged-isolated side writes, checks
all responses before committing positions/cache and preserves state on rejection.
The parallel native futures are polled together; on rejection/drop, outstanding
native futures stop, whereas independently created JS promises can continue.
Typed snapshots omit executable fields and source aliases. Update price/amount
must be native numbers; source null updates are admitted and natively refused,
with a named witness before transport. The JSON-only response ingress profile
and UTF-8/depth limits are inherited from order_services.

`tests/order_rest_oracle.cjs` executes the exact maintained method initializers,
actual enums/constants/numeric helpers and real DefaultStore. It freezes input
bytes BEFORE source Store callbacks can mutate fixtures. Its independent floor
is52 cases over11 method families, each checked on encoded/native/transformed
adapter paths. Native IDs, callback option fields, provenance, position/leverage
and reconciled orders are asserted before wire trace comparisons. Additional
actual-source/native witnesses cover null-update refusal, delayed parallel side
writes and immediate rejection dispatch. Four further source/native fixtures assert signed-zero clamp bits using shared math_min/math_max. Null protective-response rows retain the source strict property-access failure before emission. Formatter capability fixtures isolate
these orchestration methods; the real formatter implementations retain their
prior oracle evidence. Full exchange wiring and live venue behavior remain pending.

`src/order_reads.rs` ports eight order readers: the active pagination and retry
pipeline, normal/TPSL/trigger wrappers and historical orders/fills. The real
existing order/history mappers and inline TP/SL fallback helper consume retained
rows directly, without serialization. Active pagination requires an empty page;
49 populated pages followed by empty succeeds, while50 populated pages refuses
without returning a partial snapshot. Cursor type/progress and extra-query spread
precedence follow the source. Retry reads start all three families and preserve
Store orders on complete failure, returning owned snapshots. The concrete native
adapter retains independently running requests after a reader rejects. Clock callbacks
are infallible/non-mutating and retry sleep takes the exact native Number argument.
History reads preserve the source's distinct code checks, public/unknown-market
shortcuts, seconds/milliseconds normalization, strict fill numeric aliases,
contract-sized quantities, per-fill-ID dedupe and raw-empty-only fallback.
Native JSON parameters explicitly refuse nonfinite history queries/fill limits
or normalized-time overflow; they never silently substitute null. Typed Execution
refuses nonstring commission assets or inherited side-table objects. Other typed,
UTF-8/depth, prototype and snapshot boundaries inherit the existing mapper/ingress
profiles. Adapter construction, full class wiring and websockets remain pending.

`tests/order_reads_oracle.cjs` executes actual maintained method ASTs and genuine
source dependencies, including real DefaultStore. Input capture precedes mutation.
Its floor is39 cases on six adapter paths (three body phases, each pending and immediately ready) =234 native comparisons, with three
additional source/native refusal witnesses. Native order, history, execution and
unchanged Store fields precede request/clock/log comparisons. Fixture futures yield
once per request to observe all-family dispatch; real scheduling is not claimed.

Order-read review regression fixtures retain the original instrument-ID fallback type: an unmapped numeric17 does not equal symbol string17, while own numeric-key market lookup and string fallback still work. The GET await yields before handling either fulfillment or rejection so immediately-ready adapters start all three order families before processing responses. Rejection still drops outstanding native futures as documented.

The position-read slice adds nine standalone configured REST methods in src/position_reads.rs. PositionReadHost shares mode cells, post-helper auth state, per-batch disposal state, console warnings and the original-error-plus-hint emitter. Store fallback/transport row identities become owned values; nonfinite history query times, unrepresentable metadata symbols and positive-length array-like balance rows have actual-source witnesses. Income range forwarding and fresh journal record orchestration add no local behavior choice and are classified ported; reused dependency contracts remain on their own rows. Positions/metadata/balance and income/closed axes keep the source cache ordering,20-symbol batches, first-ID uniqueness, historyId versus positionId dedupe, client-side ledger window,100-page bounds and source-specific error handling. Journal mapping never applies market contractSize to already-base closePositions. Source/native fields, cache and Store checks precede request traces; the floor is56 fixtures x6 body/scheduling variants plus4 boundaries x6. Full exchange lifecycle, account validation, API creation, markets/OHLCV and websocket consumers remain pending.

Position-read review regressions preserve lodash first-value signed-zero sums and capture selected history market IDs before the first await. Reentrant market replacement/removal leaves later query IDs fixed while income/journal symbol and quote resolvers observe current Store. Native sign bits are asserted before wire traces.

HTTP receipt forwarding preserves the effective configuration after inner signing.
ACCESS-SIGN auth classification reads that configuration, including header removal;
outer retry-unwind response handling retains the final successful configuration.
Input headers are not evidence of what the adapter actually returned.

Concrete REST signing reads `VIRTUAL_CLOCK.get_current_time()` and private
websocket authentication reads `get_current_time_ms()`, matching the two source
clock calls. Lifecycle clocks and transport deadlines remain runtime operations.
`tests/native_clock.rs` sends through the production REST adapter and an opened
private socket with a deliberately different host wall clock and a pushed
40-second server offset; the shared Bitunix audit control runner independently
replaces each signing clock with wall time and requires a compiled failure.

Native public/private ping sends copy the venue's already sampled `core.ping_at`
into `BaseWebSocket::set_ping_at` before sending. The base pong handler owns latency
and ping timers; do not sample the clock again or leave it reading a detached zero.
`native_ping_latency` opens the actual constructed exchange sockets, checks both
pong latencies and pushes private orders/fills/positions/account frames into the
production handlers, asserting Store contents and the complete emitted fill.
The baseline ping regression fails at latency 0 versus 13 ms; all five scoped
checks pass after the timestamp forwarding.

Already-gone normal cancellation response IDs use shared ECMAScript String
coercion before the native Store key lookup, matching TS DefaultStore's object
keys. This differs from protective cancellation's strict root-ID comparison,
where numeric123 cannot remove string123's legs. `tests/native_cancel.rs` drives
public cancel_orders through the real signed REST host and Store for eight
actual-source cases, including nonstring normal IDs, a falsy-ID refusal and
numeric/string protective controls. The optional goneIdLookup oracle mode uses
actual cancelOrders/cancelNormalOrders/cancelAlgoOrders and TS DefaultStore;
the original53-case orchestration corpus remains unchanged.

Money-path cancellation receipt: native_cancel compiles/fails on master with
stale string123, then passes all8 source cases after normal-ID coercion. The
14 existing order_rest tests, native/Wasm all-target strict Clippy, required
all-features inventories (cancellation6+export-ignore, override7+live-TS-ignore,
binding25), parity7459/16581 and extraction0 pass after normal master refresh.
Logs: /tmp/blofin-gone-{native-red,final-native,native-clippy,wasm-clippy,
inventories,parity,extract}.log. No reviewed fixture or source corpus replaced.

The constructed ticker census in `tests/native_ping_latency.rs` drives 64
field-independent cases through 192 real public ticker/funding frames. Null,
strings, NaN-producing values, malformed objects, overflow, signed zero and
false are checked with distinct good rows and finite Store resets. All native
Ticker numeric bits match the constructed TS exchange and DefaultStore. The
shared control runner misroutes the live last-price field and requires compiled
failure followed by exact-restored success.

Model `info` uses canonical `RetainedJsonInput` inside its existing presence
wrapper. Read numbers and UTF-16 fields directly before any JSON report; source
computed numbers use `Float`. Consumer coercion uses the shared retained Number,
parseFloat and String helpers. JSON rendering belongs at the explicit output
boundary, with no alternate model-info carrier.

The concrete native secondary-key adapter launches each configured REST request
as an exchange-owned shared operation, preserving its original RestFailure.
Caller rejection drops only waiters. The real registry proxy regression holds
both sibling order responses across all three retries, then observes all nine
response-interceptor events after the first requests have settled.

Ticker REST responses, TickerInput, and the lifecycle handoff retain canonical
Input through the final row info clone. The real native exchange regression in
reads.rs exercises encoded and both native response phases, observing D800 text,
DC00 keys, Infinity, negative zero, NaN and own undefined before JSON. Its
maintained full-class source capture uses only terminal Axios responses.

All 26 boxed async SDK returns use shared `eager::start_eager` with the existing
Rc-owned inner exchange. The source synchronous prefix therefore runs when the
public method is called, before its returned waiter is polled. The runtime owns
the continuation; native DTOs and placement tokens are not serialized through a
second carrier. `divergence_timing_binding` compares the actual public mode call
with the full maintained TS constructor: three log events before return, then
the success log and confirmed fetched-leverage mode after the actual HTTP reply.
Its master regression compiled and failed with zero prefix logs versus three.

## Order-family continuation ownership

`fetchOrdersWithRetry` creates three independently owned family operations using
`BlofinExchangeHost::start_async`. The concrete NativeHost delegates to the
shared typed `start_eager` boundary, retaining `Result<Vec<Order>, ServiceError>`
as a native value. Its Promise.all-style join owns only the waiters: rejecting
one family does not cancel successful siblings or their subsequent pages.
`tests/divergence_timing_binding.rs::rejected_order_family_does_not_cancel_a_sibling_pagination`
compares the real SDK, signed HTTP and Store against the whole maintained class.
The failed normal family enters retry backoff while TPSL still requests both
following cursors. No virtual time advances to complete those sibling pages.
The generic complete-host fixture checks composition with its bounded executor;
only the actual NativeHost regression claims scheduler ownership behavior.

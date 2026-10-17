Historical tool locations use portable variables below: DEPENDENCY_ROOT is the
frozen dependency installation root, YARN_CLI is the Yarn 1.22.21 JavaScript
entry point, and CARGO_BIN is the historical Cargo executable. This rendering
does not claim that historical measurements were rerun.

# Bybit implementation checkpoints

## Client and socket composition, 2026-10-13

Integration `2f16b51f28` merged at `25cf96fea3`; both parent CI inventories are
retained (72 and 74 named step occurrences become 75, none missing). The exemption
union was parsed with tomllib before committing: 284 blocks, 284 distinct targets,
zero blank reasons. Bybit dependencies remain intact and the layer checker passes
at 37 classified crates. The combined CI wiring audit passes 195 tests.

The private BybitExchange composes BaseExchange, real primary/secondary clients,
the five-RPS primary wrapper, independent unlimited client, and two core socket
slots through WsExchangeParent. Its request submission uses the primary client
with an Untransformed POST payload and keeps complete response/failure envelopes.
The actual TS constructor, actual base secondary-client method and actual
createApiWithSecondaryKey helper produce the 12 success/factory-failure traces.
Injected API and socket constructors delimit this test: concrete API/protocol
factories and source class field initializers are not claimed by it.

The Rust side additionally executes the limiter, read-key routing, primary-only
writes (including submissions after a failure), disposal and weak ownership with
real DefaultStore/BaseExchange/BaseWebSocket. Changing the limit from five to six
compiles and fails at 101 naming `sixth primary request must wait at five RPS`;
restoration passes. All 44 library tests, including ignored live Node tests, pass.
Fresh host strict all-target Clippy and full wasm-target strict all-target Clippy
both exit 0 after the shared candle-cache fix; neither uses `--no-deps`.
The merged parity checker initially refused the pre-existing conditional module
attribute on submission and then found its unclaimed `pub` helper. The lint
attribute now lives inside the file and the internal helper is `pub(crate)`;
no status or exemption changed. Parity then passes at 6,984 rows / 1,384 public
Rust items.

No constructor, class, submission or onOpen row is promoted by this WIP. API
response decoding/diagnostic composition, concrete protocol factories and state
initializers remain implementation work. Census: 99 ported / 16 divergent /
405 pending, plus four journal pending.

Target scope: historical unqualified Clippy results below are host-target runs.
Historical `wasm check` results are compilation checks, not wasm Clippy. Neither
establishes strict lint coverage of code selected only on the wasm target.

## Target-specific lint audit, 2026-10-13

At `2e5939c4de`, `cargo clippy --locked --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-bybit --target wasm32-unknown-unknown --all-targets --jobs 2
-- -D warnings` exits 101 before reaching Bybit, on two `collapsible_if`
diagnostics in candle-cache's `indexed_db.rs` at lines 202 and 207. These shared
dependency findings are routed to the owner; no shared source is edited here.
The same command with `--no-deps` exits 0 and lints Bybit's own wasm library,
tests and examples. This narrower pass does not close the full-command failure.
Separately, the host command (without `--target` and without `--no-deps`, retaining
`--locked --all-targets --jobs 2 -- -D warnings`) exits 0 on the same tree.

The accompanying production integer-cast sweep found two float conversions:
`BybitBboLevel::try_from` checks finite, integral and 1..=5 before `as u8`;
`get_bybit_order_execution_params` checks finite, integral and 1..=10000 before
`as u16`. The other two integer casts index a hexadecimal table from masked
bytes. Timestamp floor operations in history_mapping and primary_fills retain
f64 rather than converting derived values to integer slots. This audit found no
equivalent flattening/saturating slot cast in those Bybit sites; it is not a
claim that every numeric operation has been proved equivalent.

## Exchange snapshot selection, 2026-10-12

Core 7518bacdc8 merged at a29414a1a7. Three state-independent BybitExchange
methods are extracted into exchange_state, with explicit target declarations in
the manifest. This does not claim constructor, HTTP orchestration or placement.
The token currently cannot carry Bybit BBO/TickSize extensions; this gap was
reported to the core owner rather than bypassing preflight. The concrete
cancelSymbolOrders extension will retain cancel_tp_sl beside the base trait hook.

The cargo-invoked live TypeScript oracle compares 968 cases: reliability weights,
placeholder replacement, first-seen ID order, exact ties and timestamp precedence,
plus JSON Number/string coercion and BOM/NEL boundaries. Replacing strict score
comparison with >= fails this oracle at exit 1; restoration passes. Six Bybit
live-source cargo tests pass, including the existing table/origin, response,
API/site and shape gates. Scoped plain Clippy and wasm check pass; extractor
reports zero changes/additions/orphans, parity check passes (6,901 rows, 1,014
public items including the merged core). No live endpoint or credentials used.

## Fallible coercion and tickers, 2026-10-12

Own detached review returned DO NOT MERGE on the selector's object conversion:
JSON {toString:null} throws in JS rather than producing a fallback. Helpers now
return Result and propagate this source exception; dedupe preserves its singleton
early return and otherwise evaluates every row, including unique IDs. The oracle
checks each helper and dedupe outcome independently, so one error cannot mask
another method incorrectly continuing. It now covers 1,041 selection cases.

The extracted parseTickers mapper adds 105 cases: cached fallback versus empty
list, unknown symbols, expiring funding, JS decimal-prefix parsing, signed zero,
nonfinite results, null-row rejection and property-key coercion. Signed-zero
flags are compared separately from JSON's zero normalization. Disabling object
coercion rejection and inverting expiring funding each fail the oracle at exit 1;
restoration passes. Six live-source tests, scoped Clippy and parity check pass
(6,901 rows, 1,015 public items on this inherited core snapshot).

## Ticker review correction (2026-10-12)

The independent review found numeric JSON spelling and inherited market keys.
Number coercion now uses shared safe_cex_num::js_number_string for JS
Number-to-String; 42.0 and -0.0 wire spellings are exercised explicitly. Restoring
serde Number display fails the live deepEqual assertion. The source oracle also
proves constructor, toString and __proto__ produce malformed source tickers while
Rust skips non-owned markets; parseTickers is consequently divergent, not ported.
The restored oracle passes 1,041 selection and 111 ticker cases.

## Batch request formatting (2026-10-12)

The live source oracle extracts both trigger helpers and formatOrderForBatch,
runs the source numeric utilities and source enum/table declarations, and compares
502 requests including missing market/ticker and invalid conditional failures.
Take-profit defaults are intentionally asymmetric: triggerBy uses last while
direction comparison uses mark. Changing the latter to last fails the oracle;
the restored implementation passes. The Rust Result carries diagnostics instead
of emitting logs; the adapter owns that effect. This pure slice takes the fetched
positionIdx explicitly, and does not claim async position lookup or submission.

## Typed execution option controls (2026-10-12)

The 6,576-case live-source oracle independently fails when simultaneous-extension
rejection is removed and when BBO/TickSize fields are omitted from serialization.
Both mutations compile and fail deepEqual, then the restored oracle passes. An
additional native test rejects NaN and both infinities in BBO levels, percentage
slippage and tick-size slippage; these inputs cannot be carried in JSON fixtures.

Own formatter review found that counting errors did not check their messages:
the source catch prefix and missing-ticker property error differed. The oracle
now compares full diagnostic arrays across 518 cases. Rust reproduces the live
source messages, including conditional rejection and all last/mark/index missing
ticker paths. The original 502-case pass was insufficient evidence for diagnostics.

A further own-review case uses symbol constructor without an owned market. TS
hits an inherited object property and fails on precision.price; Rust reports an
absent Market. The formatter row is now divergent for this KEY SET difference,
matching the earlier ticker boundary. Three inherited keys are compared both
ways, raising the live formatter corpus to 521 cases; ordinary-key diagnostics
remain exact. Numeric key spelling is not part of this divergence.

Detached verification at e5ef87a96e returned MERGE: the formatter's three
prototype-key outcomes are pinned and all 521 cases pass. Both ticker and
formatter differences concern key-set MEMBERSHIP, not order or numeric spelling:
Rust excludes inherited prototype entries while retaining owned market keys.

## Request interceptor WIP (2026-10-12)

The private api_request slice compares 480 request cases against the request
interceptor extracted from live createAPI, with the installed qs and AxiosHeaders.
It covers ordered/nested query encoding, requestId replacement before signing,
public-route tagging, KLINE header clearing, remote/public-only signing bypasses,
case-insensitive header precedence and Axios control-character sanitization.
Signed POST bodies retain the exact JSON bytes supplied to the signer; numeric
spelling uses the shared js_number_string. Nine live response-interceptor cases
pin both complete auth literals (including U+2014) and absent/empty/custom reasons.
Mutating the auth dash, JSON numeric formatting to Display, or requestId each
compiles and fails the live equality assertion; restoration passes.

This is NOT a factory, constructor or submission claim: all remain pending.
The private code has explicit temporary dead-code expectations until connected.
Its current input boundary is object params and JSON bodies, not arbitrary JS
objects, Date instances, undefined properties or lone UTF-16 surrogates. GET
transport serialization and the full request/response lifecycle are still to be
connected and checked. The transport response currently lacks statusText: an
absent non-auth reason yields bare HTTP status while the source uses a supplied
reason. Auth reasons are literal on both paths. A future direct-path phrase can
inherit the source's health-pattern collision; do not invent a reason phrase.

CI placement was corrected while adding this oracle: bare workspace imports in
the existing Bybit live tests are unavailable in the native job's isolated
TypeScript installation. Live tests are explicitly ignored by ordinary Cargo
and mandatory through yarn venue-oracle:bybit in the types job AFTER the frozen
root install. This runs all targets with --include-ignored. The workflow audit
pins the command, script, order, and rejection of removed/masked/disabled steps.
The existing sdk-rs/** and packages/safe-cex/** event paths already cover it.

## Header path boundary (2026-10-12)

Measured with Node v24.20.0 and Axios 1.20.0, the version selected by yarn.lock.
Raw Node validateHeaderValue rejects C0/DEL controls, but Bybit's signed and KLINE
interceptors both construct AxiosHeaders BEFORE adapter selection. That shared
constructor strips C0/DEL except TAB and trims SP/TAB; the Rust helper reproduces
that common source operation, not a direct-only restriction.

Ten cases run both live interceptor branches, then the AST-extracted HOW
newHeaders projection and the real Axios HTTP adapter against a loopback server.
Controls/OWS are already cleaned in both paths. Interior TAB and NEL survive both.
BOM and astral characters survive the HOW projection but are removed by Axios's
additional direct-adapter ByteString filter. This is not an end-to-end tunnel
claim or a Rust direct-adapter parity claim. Keep the extra direct filter OUT of
the shared helper: a compiled mutant placing it there fails the new Rust equality
check; restoration passes. Independent review found no issue with this boundary
or the test method. No production code or manifest claim changed.

## Submission diagnostic consumer (2026-10-12)

Merged reviewed core 6922766a6c and production Store/batch contract 759bd52c8b.
Per-parent rust.yml named-step counts were 55/54 -> 55 and 55/55 -> 56;
no parent step was lost. Repaired layer policy and LoadedChunks were retained.

Two helper rows now map to exchange_errors: formatBybitTradingAgreementError and
getBybitPayloadMessage. A mandatory ignored-by-default Cargo test runs an AST
extraction of the live TS helpers over 2,766 payload/exchange cases, comparing
complete strings. It covers null versus blank first fields, fallback precedence,
all four message keys, HyroTrader remediation, code coercion including unsigned
radix strings and empty strings, and JS whitespace/trailing punctuation.
Replacing JS trim with Rust trim compiles but fails at exit 101 on the BOM case;
restoring passes. Independent review found no correctness issue.

The constructor's shared client composition still needs the reported core repair:
HttpClient returns HttpFailure with response/request/context, while the secondary
key factory accepts RestAdapter and its error type cannot hold the response.
submitCreateOrders consumes err.response.data separately from err.message; those
must survive together. No response-as-success or side-channel workaround was
added. Factory, constructor and submission rows remain pending.

Validation: full Bybit oracle, strict Clippy, wasm check, clean extractor and
parity check pass. CI wiring/layer audit passes 104 executions (78 layer fixture
executions included). Census is 6,904 rows / 1,128 public Rust items on this merged
tree; Bybit itself is 87 ported, five divergent and 428 pending, total 520.
Only the two diagnostic helper statuses changed in this increment. The source
boundary exercised here is JSON data, not arbitrary JS objects/prototypes.

## Socket dispatch and send slice (2026-10-12)

Seven method rows now use core BaseWebSocket operations: public subscribe, ping,
diagnostic send/receive and dispatch; private subscribe and ping. This is six
ported rows and one divergent row, not a completed socket class. Dialing,
constructors, authentication and reconnect lifecycle remain pending.

The mandatory live oracle exercises 224 dispatch cases, 24 subscription cases,
eight send cases and 14 diagnostic filters. Three source-pinned differences cover
inherited constructor lookup, lone UTF-16 surrogates and nonfinite JSON numbers.
Finite numbers round to binary64 before callbacks, preserving signed zero.
Captured callback arrays retain source mutation behavior: appends execute,
self-removal skips the shifted successor and map removal preserves captured lists.

Four compiled mutants fail at exit 101: omit binary64 rounding, omit ping marking,
omit diagnostic emission and snapshot callbacks. Restored focused tests pass 4/4.
Independent review found the numeric mismatch and missing side-effect assertions;
both were repaired and its follow-up found no new findings. Full Bybit live oracle,
strict Clippy, wasm check, parity check/report/progress, extractor and CI wiring
pass. The extractor initially caught metadata ordering in these two manifests;
canonical ts-before-rust_files order restores zero changes/additions/orphans.
CI audit runs 104 executions. Census: 6,904 rows / 1,135 Rust items; Bybit is
93 ported, six divergent and 421 pending, total 520 (journal scope additional).

Core socket-start merge fd667fbe8d: named CI steps 56/55 -> 56, no loss from
either parent. Full Bybit oracle and 104 CI audit executions pass after merging.
The core owns pending dial task cancellation; venue integration uses that entrypoint.

## Core-owned dialing (2026-10-12)

Public/private connectAndSubscribe operations now select routing, sticky fallback,
variant URL overrides, demo/site headers and request IDs before calling core
start_websocket. Private payloadSigner refuses dialing; relay URLs retain their
query without adding requestId. No venue task or disposal hook was introduced.
The two rows describe extracted connection operations, not constructor/onOpen,
authentication, snapshot or resubscribe integration, which remain pending.

Live AST source supplies 720 cases and 3,360 actual source dials across six
attempts each. Rust runs the cases under deferred and inline scheduling and
checks direct host dials and relay TunnelHost.connect, including actual headers.
The initial test confused the private preflight query with the core query and
used a malformed fixture relay URL; own detached review proved skipping every
private relay start stayed green. The repair observes the real host connection,
uses ts-relay, and requires the core query separately from preflight.

Compiled mutants fail at 101 for relay query corruption, removed signer refusal,
omitted start, selectively skipped relay startup and a venue-owned async dial.
The last control disposes before the first poll: it proves ownership rather than
merely that startup exists. Restored tests pass, and detached review returns MERGE.
Full Bybit oracle, strict Clippy and wasm pass. Extractor initially found inherited
base.ws array whitespace from core; its owner fixed dc8cd44d5f, cherry-picked here
as 0615c44dc5. Extractor then reports zero changes/additions/orphans; parity check,
report and progress pass. Census at this slice: 6,904 rows / 1,138 public items;
Bybit 95 ported, six divergent, 419 pending (520; journal scope additional).

## Execution amounts and local WS auth (2026-10-12)

fill_event ports three rows on the JSON order-frame boundary. The live oracle
covers 1,406 cases after review added string '-0' (JSON.stringify erased numeric
negative zero in the original inputs). Execution price/quantity precede order
price/quantity, finite zero survives, camel-case presence precedes snake-case,
and poison fallback values are coerced only when needed. Replacing execution
price with order price or treating zero as absent compiles and fails at 101;
restoring passes. Detached review returns MERGE. This is cumulative quantity;
incremental fill emission remains a separate pending consumer.

Local WS authentication has 690 exact live-source HMAC/wire cases but NO auth row
claim yet. The source signs Number-to-String(expires) and sends toFixed(0)(expires),
which differ for fractional values. Both views are retained. A rounded-preimage
mutant and Rust ties-even formatting mutant each compile/fail101; restored tests
pass. Detached review returns MERGE. Remote WS auth is blocked on transport's
shared signer union: the existing HTTP-only request/response cannot carry wsAuth,
extraData.expires or wsAuthSignature. No HTTP-config encoding or side channel.

Full Bybit oracle, strict Clippy, wasm and parity extract/check/report/progress
pass. Census: 98 ported / 6 divergent / 416 pending in 520 venue rows, plus four
pending journal-adapter rows. Total corpus remains 6,904; check indexes 1,141
public Rust items. No constructor, submission, full auth or lifecycle claim added.

## Accepted-input audit (2026-10-12, through 313dd65de8)

Scope: every locally deserializable public literal enum and record at this
revision, plus the shared `BybitSiteId` alias and numeric `BybitBboLevel`.
Endpoint/order-status/side/time-in-force tables are JSON data, not enum decoders;
their existing live table oracle remains separate. The Plain/Bbo/TickSize custom
options decoder retains its existing source-driven rejected-input corpus.

The nine literal inventories had no extra string spelling. All eight string
unions nevertheless accepted externally tagged objects: seven local enums and
the shared site enum. Seven private wire enums now own the canonical names once;
public decoding requires String first, and exhaustive conversions connect the
public enum to the private wire enum. The live AST/Serde metadata checks include
aliases, casing, whitespace, prototype names, nonstrings and object tags for
every variant. Numeric levels remain exactly 1..5. Together with formatter
params this produces 510 accepted/rejected probes, not merely valid round trips.

`BybitOrderExecutionParams` additionally accepted arbitrary level/mode strings,
explicit null in optional non-null fields, and positional arrays. It now requires
a map, closes level/mode values and distinguishes absent from null. This is a
formatter OUTPUT record; the placement input already had its numeric level
newtype. It was not an unrestricted field inside ValidatedOrder.

The follow-on sweep found eight more derived records accepting sequences:
BybitApiKeyPermissions, BybitApiKeyInfo, BybitTransactionRecord,
BybitTransactionHistoryOptions, BybitRateLimitAccountRates, BybitRateLimitRow,
BybitResponseMeta and BybitExecutionAmounts. Each now has a map-required public
front gate and a private remote companion. Public qualified-call regressions
accept valid maps and reject their positional arrays, empty arrays and null.
Six records use live TS shape fixtures (including two inline member records);
response metadata and execution amounts use explicit valid object fixtures.

The first repair used remote=Self and exposed a permissive public inherent
method. Detached review proved the bypass, as well as tuple params acceptance;
both were fixed before publication. Public `Type::deserialize` calls are tested
explicitly, so importing the trait does not mask a future inherent-method bypass.
No private wire enum or remote companion is public/pub(crate).

Compiled failing-then-restored controls: add an alias; bypass the string front
gate; accept arbitrary nonempty level strings; coerce null to a default level;
reintroduce a public permissive inherent decoder. Each fails at exit 101 in a
named acceptance test. Independently bypassing each of the eight record map
gates also fails the public-record test at 101 while naming the affected type.
The metadata oracle is paired with exhaustive public matches/conversions so a
serde-skipped Rust variant cannot disappear without compilation review.

Known remaining gap: the shared ExchangeOptionsBybitSiteId still accepts
`{"BRA_BTL":null}`. Its owner has the repair routed; the local BybitSiteId row is
now divergent and a test explicitly pins the gap until the upstream repair
arrives. This audit does not claim the entire accepted surface is clean. Measured
venue census: 97 ported / 7 divergent / 416 pending; journal adapter remains four
additional pending rows. The one-row drop is the shared-site claim correction.

The record review then found optional-null/scalar widening in BybitResponseMeta.
Presence-aware decoding now rejects null retCode/retMsg/rawResponse and primitive
rawResponse values, preserving arrays: the actual TypeScript compiler says
Record<string, any> accepts arrays. The maintained generator asks the checker
about 28 candidate assignments to the live source declaration, so this boundary
is not inferred from the word Record. This brings the AST/compiler accepted-set
corpus to 538 cases. Outer records still require maps. The mapper itself was not
changed to narrow its source runtime behavior.

Final detached review at 9df7a81579: MERGE, 24 focused tests. Its independent
controls widened the live TS retCode declaration to number|null and narrowed
Rust rawResponse to maps; both failed at 101, restored green. Local controls also
proved null coercion and primitive rawResponse widening fail. Final scoped
all-targets/include-ignored tests, strict clippy, fmt, wasm check, parity check,
extractor check (zero edits/additions/orphans), report and progress passed.
Local validated progress is 793/6904; it measures indexed manifest claims, not
behavioral proof. No CI steps or shared types changed in this audit.

## Shared coercion merge (c99591729a, 0022441914)

Merged the authorized utils head 5cf6b4d8d8. Named rust.yml steps from the two
parents were 56 and 55; result 56, none missing from either parent. Only formatter
and exemption hunks conflicted: kept the signed-byte documentation, delegated the
formatter to the single types-layer implementation, retained LoadedChunks plus
all coercion exemptions. CI-wiring audit: 104 passed.

Bybit's String(any), parseFloat and response Number(string) wrappers now consume
the shared primitives; 119 duplicate implementation lines removed. Fallible
String errors propagate with their source message; no fallback or panic added.
Substituting Number(string) for parseFloat compiles and fails the maintained
execution source oracle at 101, restored version passes. No rows were held solely
on those primitives and no new venue claims were made.

Full Bybit all-targets/include-ignored tests, strict clippy, wasm, extractor
(zero edits/additions/orphans), parity check/report/progress passed after merging.
Selected shared numeric coercion/number-string tests also passed. Local merged
measurement: 6904 rows, 1176 public items; validated progress 825/6904. Bybit still
97 ported / 7 divergent / 416 pending plus four journal rows pending. Detached
review independently checked the merge and live response/selection/execution
oracles and returned MERGE.

### Public ticker events (2026-10-12)

The live AST oracle runs 1,458 linear/inverse/spot cases through the real
DefaultStore, comparing all eleven numeric fields, signed zero, complete errors
and notifications. It explicitly witnesses the current shared JSON patch's
nonfinite refusal (retaining the prior number) and nine inherited-key membership
differences. The three handler rows are divergent FINDING claims, not clean
parity claims; replacing this patch with the forthcoming typed operation must
update the witness and notes. Substituting Number(string) for parseFloat compiled
and failed the named test at exit 101; restored implementation passes. Strict
clippy and parity check pass (6,904 rows, 1,177 public items). These functions do
not claim constructor, authentication or subscription lifecycle integration.

Own review found that single malformed fields did not prove atomic patch application:
flushing accumulated fields on a later coercion error compiled and passed. The
corpus now includes valid prefixes followed by a throwing value at every later
field, across all three variants; production already deferred the Store update.

### Family REST ticker mapping (2026-10-12)

The existing live exchange-state oracle now extracts parseTickers separately from
linear, inverse and spot: 333 ticker cases alongside 1,041 position-selection
cases. Inverse shares the linear mapper; spot forces fundingRate/fundingTime to
zero. All three pin inherited market-key membership explicitly. Substituting the
perpetual funding rule for spot compiled and made the live oracle exit 1;
restored implementation and parity check pass. This does not claim REST fetching.

Own REST ticker review found two oracle gaps, not production defects: JSON
collapsed NaN and Infinity together, and all-fields-malformed inputs masked
whether funding was lazily parsed. The oracle now records nonfinite classes
before serialization and varies each field independently: 3,213 ticker cases.
Both plausible substitutions (Infinity to NaN; eager funding parse before the
spot/expiry guard) compile and fail the live comparison at exit 1. The original
333-case corpus passed both substitutions; it did not defend these claims.

### Private market and candle mapping slices (2026-10-12)

Market callback extraction covers 549 cases from each family's actual
fetchMarketsHelper mapping expression: status, category, raw metadata, first fee
match, fee failure ordering, numeric boundaries and competing coercion failures.
A spot-PreLaunch-is-active substitution compiles and fails at 101. Candle callback
extraction covers 1,236 sequences from the three listenOHLCV methods, comparing
emission AND retained state after each call, full errors and nonfinite classes.
Replacing the source high comparison with f64::max compiles and fails at 101.
Restored tests and strict clippy pass. Both modules remain private and their
fetch/subscription rows pending: these tests do not claim request scheduling,
trade-derived candles, reconnects or timers. Market's typed text fields reject
non-string symbol/base/quote values; no whole-method conformance claim is made.

### Private orderbook state slice (2026-10-12)

933 sequences execute each family's live parser and the live shared orderbook
helpers. State, emitted books, exact errors and resync requests are compared after
every frame, including partial mutation before a later throw, string update IDs,
sequence gaps, suppression while resyncing, and unsorted snapshots. Inserting a
sort before publishing compiled and failed at 101; restoration and strict clippy
pass. The private slice accepts array level collections; unusual array-like
objects/strings remain outside its implemented boundary. Subscription rows stay
pending: there is no claim for resubscribe sends or timer ownership yet.

Book review found an element-level indexing defect inside supported array
collections: a string row or numeric-key object was read as an empty array,
producing NaN. Row property lookup now handles both, including UTF-16 indexing
for strings; the corpus is 981 sequences. This is a production correction, not
only corpus widening. The separate collection-level restriction remains.

### Order-history fallback mapper (2026-10-12)

380 live-source cases independently exercise the linear and inverse methods:
nullish field precedence, lazy price/fee fallbacks, timestamp units and bounds,
filled-substring status behavior, fee details, first duplicate retention and
stable descending order. Last-duplicate-wins substitution compiles and fails at
101; restored test and strict clippy pass. Two boundaries are declared on both
rows: non-string output fields refused by typed Execution, and a fee-detail JSON
text with a lone UTF-16 surrogate losing otherwise valid sibling fees under
serde. Both have explicit source and Rust witnesses. Fetch/pagination remains
pending; closed-PnL partial-close grain is not silently substituted for fills.

Oracle scope audit (2026-10-12): 20 .mjs scripts including the journal fixture; only check-unions.mjs constructs a
TypeScript Program. It compares each initializer type with the live declaration,
not the annotation of a pre-widened expected variable. Literal inventories come
from source AST literals. No expected as-const tuple/index-type comparison was
found. Runtime oracle comparisons use values; the recorded semantic mutations
above fail assertions rather than relying on compile-time assignability.

History review correction: the initial annotation edit matched a dot-qualified
name, but both source members use `#`. The two rows were still pending despite
the paragraph above; the exact private-member rows are now annotated and
parity-check passes. Measured venue census: 97 ported, 14 divergent, 409 pending.
The same gate run required moving dead-code lint attributes inside the three
private slice modules (module cfg_attr is rejected by the inventory) and making
BookState methods crate-visible consistently with their owning private type.
No gate or exemption was changed to accommodate these slices.

### Journal intermediate work, not a canonical adapter claim (2026-10-12)

369 live-source cases compare insertion-ordered buckets, numeric entry-price
grouping, retained partial-close counts, and numeric totals against the source
fold. A last-row-PnL replacement compiles and fails the realizedPnl assertion at
101 (case 41: 0.26 versus 0.46); restoration passes. This is private intermediate
work, not PositionRecord or full dependency-callback parity. All four journal
rows remain pending on the shared canonical type. Sorting all buckets before
folding also does not yet reproduce interleaving with resolver callbacks; full
adapter integration must retain source callback and failure ordering.

### Inverse/spot REST market request sequence (2026-10-12)

143 live method cases compare exact request order/params, emitted errors, mapped
results and propagated error messages. Inverse fetches PreLaunch before checking
the first response; checking early compiles and fails request-order case 2 at
101. Restored tests pass. A pending read's Drop issues no later request and
mutates no shared state. The private operation is awaiting venue client wiring;
this is not a constructor/factory claim and changes no manifest status.

REST market review added primitive response bodies and malformed second-response
lists: the corpus is now 191. These exposed wrong emission behavior and a wrong
`data`/`data2` error name; both were corrected. The operation is now public and
maps the two fetchMarketsHelper rows as divergent at the declared typed Market
and array-list boundary. It accepts the existing GET/emitter dependencies;
constructor/factory wiring remains pending. No duplicate HTTP runtime is added.

### Primary execution mapping slice (2026-10-12)

610 live-source cases cover both linear/inverse USER_TRADES loops, known-nontrade
blacklisting, alias precedence, lazy price fallback, time units, maker typing,
full errors and nonfinite commission classes. A collect-page-then-append mutant
compiles and fails prefix-retention case 39 at 101. The original implementation
passes; no fetchHistoricalFills row is claimed yet. Typed Execution output fields
still need explicit malformed-field witnesses before a full method claim.

### REST ABI merge and historical-fill orchestration (2026-10-12)

Core ed304ce751 merged with 56/55 named CI steps on the parents and 56 in the
result, none missing. Bybit dependencies survived; scoped all-target compilation
passed with jobs=2. The dial fixture now suspends timers while its clock stays
fixed, instead of repeatedly expiring the new core relay deadlines inline.
All 720 live dial routes pass under both scheduling modes after that fixture repair.

112 live linear/inverse fetchHistoricalFills cases compare complete requests,
results, diagnostics and fallback calls. Source AST supplies window/lookback
constants. Linear alone retries an empty short fallback over 30 days; inverse
has one fallback and a distinct empty-result diagnostic. Sharing linear retry
with inverse compiles and fails diagnostics case 86 at exit 101; restoration
passes. Two pending-future drop tests prove no later recovery request starts.
This private orchestration still awaits class wiring and malformed typed-field
witnesses; fetchHistoricalFills remains pending. No manifest claim changed.

Historical-fill query windows also retain two explicit CHOSEN, unclaimed guards: nonfinite bounds return Err, and finite arithmetic unable to advance a window returns Err. TypeScript has neither guard. These require row-level divergence notes and boundary witnesses before a full fetch claim, alongside the typed Execution field differences.

The private historical-fill review returned MERGE at bfb9482a90. Its additional
compiled mutation stopping on an empty page with a live cursor fails request
comparison case 0 at exit 101; restored four fill tests and dial ownership pass.

### Complete venue-error formatter (2026-10-12)

13,830 live AST-method cases compare payload/fallback precedence and complete
messages. Preferring the transport fallback over a venue payload compiles and
fails complete diagnostic case 26 at exit 101. Restored test passes. This adds
only the pure formatBybitVenueError row, with exchange name explicit; constructor
and submission integration remain pending. Census: 98 ported / 16 divergent /
406 pending across 520 venue rows; four journal rows remain pending separately.

### Integration merge and shared site finding closure (2026-10-12)

Integration 7738215fb0 merged at 43140b4e1b: 56/55 named CI parent steps to
56 merged, zero missing. All Bybit dependencies retained; all-target no-run
compilation passed jobs=2, layers passed for 37 crates, combined CI/layer audit
104/104. Follow-up 31c6307631 removes two accidental double leaf-error wrappers
in transport test merge resolution; the new heartbeat deadline regression passes.
Global parity is known-red on exactly math_log10 and math_pow missing shared
registrations, reproduced on an archived integration-only tree and routed.

The shared site enum now rejects object tags. Its old FINDING witness failed on
merge; the replacement inventories canonical names through the public strict
path's serde unknown_variant metadata, retains an exhaustive match and rejects
object tags through ordinary and public qualified decoding. Bypassing the shared
string guard compiles and fails the named object-acceptance assertion at 101;
restoring the unchanged shared source passes. This closes the site FINDING row.
All 44 Bybit crate tests (32 library, 8 live data, 1 ticker, 3 public decode) pass
with include-ignored; examples also compile. Census: 99 ported / 15 divergent /
406 pending, plus four pending journal rows. No constructor/submission claim.

Integration a6f317fbc9 merged at ab72766194 with the same 56/55 -> 56 gate-name
union, intact dependencies, scoped all-target compilation and 37-crate layer
check. Global parity is now green (6906 rows, 1242 public items), and extractor
check reports zero updated/added/orphaned rows. Site closure received MERGE.

Bybit now delegates its remaining local truthiness and Number(any) wrappers to
safe_cex_num::js_truthy and js_number. Fallible errors propagate; parseFloat
remains the distinct shared primitive. All 44 live Bybit tests pass after this
consolidation, including full numeric classifications and coercion failures.

### Inverse position arithmetic slice (2026-10-12)

406 live BybitInverseExchange.mapPosition cases compare fields, full logs and
numeric classes, including -0 and both infinities. Uses the newly public shared
math_round; substituting Rust round compiles and fails negative half-tie case380
at exit101, then restoration passes. The fixture executes source PositionMode /
PositionSide enums and createPositionId rather than spelling their values locally.
This remains private WIP: symbol typing, inherited position-side table keys and
market/ticker state lookup need their own boundary witnesses before the full
mapPosition row can be claimed. No manifest changes from this slice.

Inverse-position review found a separate side-table coercion defect: [Sell]
must select short, and an object owning a non-callable toString must throw before
logging. The string-only implementation failed both source witnesses at101.
The repair uses shared fallible js_string only after positionIdx1/2 short-circuit;
430 source cases now pass, including both indices bypassing throwing keys. The
fixture now extracts POSITION_SIDE itself from bybit.types.ts as well. Inherited
prototype table membership remains explicitly unclaimed, separately from coercion.

## Private spot/inverse order mapper checkpoint (2026-10-12)

The unclaimed `order_mapping` slice compares 940 live AST-source cases against
both `mapOrder` methods, including their actual tables, `v`, safe-math helpers and
inverse `remainingInStoreUnits`. Comparisons cover complete outputs, signed-zero
and non-finite classes, and every clock call. Synthetic stops preserve spread
semantics: a derived take-profit can retain its parent's stop-loss trigger field.
Inverse remaining is recomputed at each synthetic stop's own price; spot copies
the parent's amount. Inverse opening triggers become conditional; spot does not.

Two compiled substitutions failed the named live test at exit 101: copying the
parent amount for inverse synthetic stops failed case 470; applying inverse
conditional classification to spot failed case 16. The restored implementation
passed. Scoped library clippy with warnings denied passed.

This is private WIP, not a claim for either whole mapper or any fetch method.
Class/store lookup integration remains outstanding. The selected `Order` model
also refuses malformed required enum/string fields, non-boolean truthy spot
`reduceOnly`, and inherited table keys that JavaScript can propagate; these
boundaries need bidirectional witnesses and explicit CHOSEN notes before claims.
The fallback subtraction currently accepts numeric/string quantities only;
other raw JSON shapes need source-driven treatment before the slice is exposed.

## Inverse position mapper with production Store (2026-10-12)

`inverse_position::map_position` now performs its own `Store` lookups. The
478-case live-source corpus uses real DefaultStore maps with IDs distinct from
symbols, absent owned markets, and a logging callback that clears tickers on
both sides. No store borrow survives that callback. Timestamp acquisition is
lazy and its call count is compared, including TypeScript failures.

The row is CHOSEN divergent for the selected typed model, not silently ported:
seven missing/null/non-string symbol probes are retained by TypeScript but
refused by Rust, and twelve inherited side-key probes return prototype values
in TypeScript but recoverable errors in Rust. Numeric position indices 1/2
bypass those side lookups on both sides. The note states the refusal's effect
on evaluation order and the typed Store field boundary. These witnesses do not
claim arbitrary prototype emulation or malformed typed Market/Ticker acceptance.

Replacing symbol lookup with ID lookup compiles and fails position case 190;
reading the clock eagerly compiles and fails clock case 18, both at exit 101.
Restoration passes all 478 cases. Scoped all-target clippy with warnings denied
passes. Constructor and fetch orchestration remain pending; this claims only
the mapper. Venue census: 99 ported / 16 divergent / 405 pending (520 rows), with
four journal rows pending separately on shared PositionRecord.

## Private inverse position pagination checkpoint (2026-10-12)

`rest_positions` remains private and unclaimed pending exchange/client and mapper
composition. The 150-case live AST-source oracle executes fetchPositionsHelper,
reinitOnThrow, getCategoryForSymbol, v and the endpoint table. It compares the
production-selected URL and parameters, mapping sequence, emitted raw values,
reinit state, results, and error name/message/response plus original error
identity. Its generic error is returned intact for request rejection; it is not
converted through Display or a message-only wrapper. Mapping is still an
injected callback here, not a claim that real mapper/client composition ran.

Account-wide venue rejection discards accumulated pages and sets needsReinit;
symbol-scoped venue rejection throws without newly setting that flag. A rejected
request sets the flag and returns the original error in either scope. Fresh
object/array cursors compare by response identity, primitive cursors by JS
strict equality. The read retains the source's adjacent-cursor check rather
than adding an undocumented global cycle guard or page limit. JSON decoding
provides fresh object identities; arbitrary reused host objects are not modeled.

Two Drop tests cancel a pending first read and a pending second page: neither
maps a partial prefix nor changes reinit state or starts another request.
Four compiled controls fail at exit 101: retaining the prefix after an account
venue error (mapping sequence case 5), choosing the wrong route (requests case
0), comparing object cursors structurally (requests case 145), and relabeling a
coercion TypeError as Error (error class case 78). The restored 150 cases and both
Drop tests pass, as does scoped all-target clippy with warnings denied.

### Request client composition checkpoint

The private RequestClient drives the existing request interceptor through the
production core HttpClient, serializer and body transform. The live source API
installs its interceptors on an actual Axios client in 48 cases (GET/POST,
create/KLINE, local/unsigned/refusing signer, absent/object/JSON-text/plain-text
bodies). The comparison checks complete transmitted headers, query and body,
including the HMAC, plus preflight refusal before common request start and its
entry into the common failure path. The outer common TypeScript factory is a
boundary stub; this does not claim its internals or the response/probe lifecycle.

A plausible late-default restoration compiled and failed at `signed wire case 12`
(exit 101): it restored Content-Type, receive-window and broker headers after
KLINE cleared them. The restored implementation passes. A separate explicit
FINDING witness records Axios adding Content-Type application/json while the
shared Rust transform leaves it absent for KLINE with object data. The witness
must flip when the core header repair arrives; no venue-local transform is added.
Signing JSON now consumes shared js_json_stringify instead of a private recursive
implementation. No constructor, createAPI or submission status was promoted.

### Integration 91db928805 migration

The Content-Type finding above is closed by the shared post-signing transform;
the source comparison now includes the default Accept header too. This exposes a
separate ordering seam: core HttpClient merges defaults after the venue cleared
KLINE headers, restoring Accept. Actual-source case 12 fails with that one extra
header. A narrow acceptance witness pins the source omission and Rust restoration;
core owns the missing default-before-interceptor operation. createAPI stays pending.
Ticker updates migrate to ModelPatch::set_number and propagate patch errors.
The live DefaultStore witness now requires non-finite overwrite rather than stale
retention; inherited symbol membership remains separately divergent.

Merge validation retained all named CI steps (77/76 parents to 77), left the
Bybit dependency manifest unchanged, and parsed 301 exemption blocks with 301
distinct targets and no blank reasons. The scoped all-target live test run passed
57 tests with zero ignored; host and wasm32-unknown-unknown strict all-target
Clippy each passed separately. Extractor drift reports 6,984 symbols across 490
files, zero updated/added/orphaned; the layer audit passes all 37 crates.

A compiling regression control replaced native set_number with a JSON patch.
The ticker oracle failed at case 4 (bid1Price true, whose parseFloat is NaN),
exit 101, because the JSON null could not carry the numeric update. Restoring
set_number passes the same test. The three ticker notes drop only that repaired
FINDING; their independent inherited-symbol divergence keeps their status.
Venue counts remain 99 ported / 16 divergent / 405 pending, plus four journal
pending rows; no constructor or factory claim follows from this merge.

The consolidation follow-up takes integration b1ef25ef36 (KuCoin and GRVT).
Named CI steps are 77/78 to 79 with no loss from either parent; all three venue
script entries survive and the merged wiring audit passes 205/205. Exemptions
still parse as 301 blocks / 301 distinct, and Bybit dependencies are unchanged.
The layer audit remains green. The shared types edit only relocates its ws_auth
module declaration; the Bybit all-target no-run build checks the merged lock and
dependency graph. Global parity before this additive merge passed at 6,984 rows
and 1,534 public Rust items.

### Shared response lookup consumption

Five mapper/fetch modules consume shared get-key v rather than fixed camel/snake
pairs. The inverse-position live corpus grows from 478 to 496 cases and exposes
a real omitted third alias: positionIM -> position_im -> positionIm. A compiled
control restoring the former exact/snake-only lookup fails exit 101 at position
case 478, where Rust drops marginSize 0.03. The shared lookup passes all 496.
Present null blocks the third spelling; an exact key wins over both aliases.
All 45 library tests including live oracles pass. Order protocol lookup also
uses shared TableHit, deleting its local twelve-name prototype list. No fetch,
constructor or submission row is promoted by this dependency integration alone.

### Existing numeric ingress findings

The 186-case live Node corpus covers 31 numeric paths: 22 paths through six
record decoder entrypoints plus nine across Plain/Bbo/TickSize placement. It
compares native f64 bits, before serialization, for -0, -0.0, zero, both overflow
signs and underflow, and adds native Value(-0.0) controls. Every existing-Value
lexical -0 path loses the sign; raw nested BybitRateLimitRow also loses it because
its child map decoder buffers again. The other raw paths, decimal -0.0 and native
Value controls preserve it. TickSize tolerance additionally refuses overflow at
as_f64; the other measured fields retain infinity. These are existing defects,
not a Value-first candidate or a deliberate correction. Seven ported rows move
to divergent FINDING, and the already-divergent placement union adds its own
FINDING separately. Five record rows cover six Rust records because the account
rate record is an explicit split target of BybitRateLimitRow. Count: 92 ported /
23 divergent / 405 pending, plus four journal pending. No ingress shim is added.

### Shared interceptor operation consumption

Integration b8fc4dfbaa supplies execute_with_interceptor. RequestClient now gives
factory headers to core, which merges defaults once before invoking the venue
callback. The previous test-only Accept removal is deleted; all 48 actual-Axios
wire cases compare full headers without adjustment. This closes the ordering
finding recorded above, not the still-pending response/probe/retry factory work.
A compiling late-default control reinserted Accept after venue preparation and
failed at `signed wire case 12` with exit 101. The restored callback passes.

### Owned private reconnect sequence

The actual source sendReconnectSequence initializer supplies 12 disposed,
reconnecting-mode and auth success/refusal/throw cases. Each runs with deferred
and inline Rust scheduling while the clock future is pending. Tests pin the
synchronous prefix, sleep before auth, full subscribe/ping frames, actual native
reconnect initiation, unchanged fallback selection after deliberate silence,
error text and snapshot presence. Source witnesses also record auth continuing
after disposal; the Rust cancellation difference is declared CHOSEN on the row.
Unpolled, pending and inline-pending disposal release the socket Weak and never
invoke auth. A plausible venue-owned spawn compiled and failed at
`core-owned reauth must release socket: unpolled` (exit 101); restored code passes.
The production operation consumes start_message_operation; protocol constructor,
onOpen and onMessage integration remain separate rows.

### Concrete private open binding

The constructor's with_private_protocol path instantiates PrivateProtocol and
calls it through BaseWebSocket::on_open. The oracle executes five actual source
methods together: onOpen, sendReconnectSequence, auth, subscribe and ping.
Eight local/remote/missing-signature/refusal and disposed cases run under both
deferred and inline Rust scheduling. Complete wire frames, signer envelopes,
pre-completion state, delay, diagnostics and native reconnect counts are compared;
disposal releases the venue even while the socket is retained. A compiling
substitution of local auth for the shared remote-capable auth operation fails
`composed auth frames` for mode signed (exit 101); the restored binding passes.
Only onOpen is newly claimed here. The auth row retains its pending malformed
input boundaries, and onMessage/full class construction are not claimed.

Validation for this slice: 48 library tests passed with live ignored tests enabled;
host and wasm32 strict Clippy passed separately. Parity passed at 7,334 rows and
2,307 public Rust items; layer audit classified 38 crates. Extractor reported
zero added/orphaned rows locally (the installed smol-toml dependency caveat above
still applies). An independent slice review returned MERGE with no findings and
reran the concrete protocol test plus both strict lint targets on the final
inherent-open wrapper. This is not current-integration merge readiness.

### Shared numeric ingress opt-in

Five closed records (21 numeric paths) now use JsNumberInput and private
map-required wire decoders, with exhaustive explicit JsNumberOutput impls.
The permissions subrecord also replaces its intermediate Value buffer. Entry
callers use shared js_from_str/js_from_value/js_from_value_ref; an existing
Value must reach these before a legacy serde rebuffer can erase its sign.
The corpus now has 279 live Node cases over 31 paths, adding decimal, exponent
and negative-underflow inputs. Repaired paths assert raw, owned and borrowed
native f64 bits; native Value(-0.0) remains a positive control. The nested raw
rate record is covered. ResponseMeta's opaque Value output and placement's
unregistered shared PlaceOrderOpts graph retain their sign-loss findings.
TickSize's explicit read now uses js_value_as_f64: overflow passes while the
upstream placement buffer's existing-Value sign loss remains separately pinned.

Two plausible production substitutions compiled and failed at exit 101:
restoring the rate-row Value buffer fails borrowed futures/classicAndUTA -0,
and restoring TickSize as_f64 fails raw bybitSlippageToleranceTicks 1e400.
The full scoped suite passed 61 tests with live oracles enabled; wiring237/237.
Restored code passes the targeted 279-case test. Independent review returned
MERGE/no findings for the opt-in and integration merge, separately rerunning
host and wasm strict Clippy and parity (7,334 rows / 2,530 public items).
CI steps retained 86/87 to 87; exemptions parsed as 323 blocks / 323 distinct,
with parent reasons retained. Layer audit passes at 38 crates; extractor reports
zero drift locally with the installed-dependency limitation stated above.

### Placement numeric graph repair

The follow-on removes placement's input and output Value buffers. Its explicit
JsNumberOutput impl exhaustively destructures all local variants and all sixteen
shared PlaceOrderOpts fields, then checks six shared numeric fields, ticks and
the bounded BBO level through an expression-macro registration. No foreign impl
or shared-crate edit is needed. New local/shared fields require compilation
review. The actual whole output is serialized natively and checked, not a numeric
projection substituted for the output guard.

All nine placement paths in the maintained 279-case / 31-path corpus now pass
raw, shared-raw, owned and borrowed bit checks. ResponseMeta's opaque Value
finding remains. The Bbo and TickSize rows return to ported; the union retains
its independent CHOSEN representation. Counts: 98 ported / 19 divergent /
403 pending, plus four journal pending. Standard serde can still erase a lexical
sign before the custom decoder runs; callers must enter through the shared API.

A compiling restoration of the old output Value buffer fails at exit 101 on
borrowed Plain amount 1e400: numeric ingress lost Number at $.amount. Restoring
the native serializer passes the same test. The full scoped all-target suite
passes 62 tests, including the existing rejected extension-input corpus and a
separate unclaimed URL-selector test. Host and wasm strict Clippy are separate
runs. Parity passes at 7334 rows / 2530 public items, and extractor reports zero
drift locally (same installed smol-toml limitation as above). A read-only
adversarial review returned MERGE with no findings; URL WIP was excluded.

### Concrete URL selectors

Four selector rows promoted independently of constructor completion. The live
AST oracle extracts the actual four exchange methods and URL tables, covering
32 name/demo/network/category combinations. Concrete private protocol routing
consumes the private getter; connection options retain the source is_demo rule.
A plausible is_demo-only substitution compiles and fails at exit 101 on the
named demo selector assertion (bybitdemo, isDemo=false, livenet, linear).
Restored code passes. The prior 62-test full scoped run and separate host/wasm
strict Clippy runs included this production code; no shared dependencies, gate
steps or layer policy changed. Read-only independent review returned MERGE with
no code findings and independently executed all 32 source cases. Counts:
102 ported / 19 divergent / 399 pending, plus four journal pending. These are
selector claims, not completed constructor/API/public-protocol claims.

### ResponseMeta named passthrough

Integration 82445b433d merged at 75079eac9d. Both parents had 87 named CI steps;
merged 87 retains each multiset. Real TOML parse: 327 blocks / 327 distinct, with both
parent target sets and reasons retained. Bybit dependencies unchanged; layer 38
and scoped locked all-target compilation pass. Incremental merged-state review
returned MERGE with no findings, checking all 31 incoming paths exact against
integration, parent lock edges/packages and additive script entries.

ResponseMeta now captures JsNumberInput before its map-required wire decoder.
raw_response: Option<Value> is explicitly registered as named passthrough and
constructed via js_deserialize_passthrough; no whole-output guard is bypassed.
The corpus expands 279 → 297 cases and 31 → 33 numeric paths with nested object/array
payloads, all measured against live Node bits on raw, shared-raw, owned and
borrowed entries. retCode's previous finding flips, giving 103 ported / 18 divergent /
399 pending plus 4 journal. Ordinary serde can lose a sign before entry; the row
requires shared whole-entry ingress and claims record decoding only.

Replacing the named passthrough with ordinary Value::deserialize compiles and
fails 101 with numeric ingress forbids serde_json::Value buffers. Restored code
passes the 297-case corpus. Read-only adversarial review returned MERGE with no
findings. Parity passes 7334 rows / 2535 public items; extractor zero local drift with
the existing installed-dependency caveat; wiring 237/237.

Final restored merged-tree run: all 62 scoped tests pass with live oracles enabled.
Host strict Clippy and wasm32-unknown-unknown strict Clippy each exit 0, run
separately with two Cargo jobs. The measured production record path is
BybitResponseMeta::deserialize -> BybitResponseMetaWire::deserialize ->
present_raw_response; factory response lifecycle remains outside this claim.

## Private auth method claim (after 7180768a0a)

The existing local/remote auth implementation is now mapped separately from the
constructor and private onMessage. The row is MIXED divergent, not ported: it
names shared malformed-response restrictions, native nonfinite expiry metadata,
the UTF-8 boundary, owned cancellation, and indirect Store notification errors.
No production auth logic or shared type was changed to obtain this claim.

Validation: all six scoped auth tests pass, including 690 live local cases,
1,150 live remote cases, actual concrete private onOpen under deferred and inline
scheduling, wrong-kind refusal and pending drop. The added boundary test runs
15 signer response cases (including accepted nulls and scalar Unicode and a
double-escaped lone surrogate), plus three nonfinite expiry cases. Source expiry
classification and Rust RawValue contents are captured at the native signer
boundary before serialization. Serialized envelope agreement alone is not the
assertion for this finding.

Compiling production mutation: replace remote signer/decode failure propagation
with an empty successful WsAuth response. The new test exits 101 at
`shared response refusal: "numeric signature"`. Restore the actual propagation
and all six tests pass again. This substitution models a plausible fallback;
it does not merely remove auth. The production file is restored byte-for-byte.

Host strict Clippy and separate wasm strict Clippy pass. Parity: 7,334 rows /
2,537 public Rust items; extractor: zero updates, added or orphaned symbols.
Counts: 100 ported / 22 divergent / 398 pending plus four journal pending.

## Constructor scalar state and two getters (after 2f2685ddd2)

Ten scalar declarations are now installed on the concrete exchange. The existing
actual-constructor oracle additionally extracts those declarations, the source
retry constant and MarginType enum, plus accountType and
supportsTradeDerivedCandles. Across 12 success/factory-failure cases, five
successful constructions each exercise 16 raw status inputs (80 comparisons).
Numeric defaults are captured as binary64 bits before source JSON serialization;
Rust compares bits, while null and undefined initial states remain distinguished.

Only the two getters are promoted. The full constructor, state-dependent fetch
methods, concrete API factory and public timer lifecycle remain unclaimed.
A compiling numeric-string coercion substitution fails at exit 101 on the named
strict-account-type assertion for string "1", which must stay UNIFIED. The
original numeric-only read is restored. Independent review returned MERGE with
no remaining findings and separately ran the source fixture.

Final restored scoped constructor test, host strict Clippy and separate wasm32
strict Clippy pass. Parity passes 7,334 rows / 2,537 public items; extractor reports
zero updates, additions or orphans. Counts: 102 ported / 22 divergent / 396 pending
plus four journal pending. This is a slice checkpoint, not current-head merge-up
readiness; integration was not merged during the announced freeze.

## Execution-query window splitter (after db2c21daff)

The source field-function and its two readonly range/lookback constants are
extracted directly. A 222-case oracle compares raw binary64 endpoint bits before
JSON serialization, with omitted endpoints represented separately. Coverage
includes exact seven-day edges, reversed and fractional ranges, large rounded
timestamps, signed zero and terminating NaN/Infinity paths. It does not claim
termination for source inputs that loop indefinitely.

The strict source loop omits a terminal singleton: [0,604800000] returns only
[1,604800000]. A named source assertion and the row's
`ts-bug:singleton-window-omission` marker retain this behavior. Replacing the
one-millisecond step with the prior window start compiles and fails case 11 at
exit 101. Restoring the production step passes all 222 cases. Independent review
returned MERGE, no findings, and independently executed the source oracle.
Only getExecutionQueryWindows is promoted; no full-fetch claim is added.

Host strict Clippy and separate wasm strict Clippy pass. Parity: 7,334 rows /
2,537 public items; extractor: zero updates, additions or orphans. Measured venue
counts: 103 ported / 22 divergent / 395 pending, plus four journal pending.

## Consolidation merge: fa84276ef4 with integration 13fe8de843

The merge was conflict-free; all 314 incoming paths are byte-identical to the
integration parent. The local Bybit crate and its manifest rows are unchanged by
the merge. No whole-side conflict resolution was used. Parsed workflow step
multisets retain both parents: all-workflow 347/349 -> 349 and Rust workflow
87/89 -> 89, with zero missing named steps. Bybit Cargo.toml and its eight direct
dependencies remain exact. Cargo's locked all-target build succeeds.

Real tomllib parsing gives 333 exemption blocks / 333 distinct targets, zero
blank reasons and zero missing parent targets. Layer audit passes at 38 crates;
CI wiring passes 247/247. All 65 scoped venue tests pass with live Node oracles
enabled and zero ignored. Extractor reports zero updates, additions or orphans;
parity passes 7,334 rows / 2,772 public items. Venue counts remain 103 ported /
22 divergent / 395 pending, plus four journal pending.

Scoped formatting, host strict Clippy and separate wasm32 strict Clippy each pass
on this merged tree. The merged-state review is recorded in the merge commit.

### Incremental Coinbase consolidation: 5ff5dd0c0d with 7a10823341

Integration moved during the reviewed merge. The next automatic merge retains all
86 incoming paths exactly. Bybit code, manifest rows and dependencies are
unchanged; the layer policy is unchanged. Named CI steps retain both parents:
349/350 -> 350 overall and 89/90 -> 90 in Rust CI. Exemptions still parse as
333 blocks / 333 distinct targets, with no blank reasons or missing parent
targets. The changed exemption reason is integration's Coinbase-specific edit.

Incremental validation: scoped locked all-target no-run succeeds; layer audit
passes at 38 crates; wiring passes 251/251; parity passes 7,334 rows / 3,198 public
items. The full 65-test, host and wasm lint results above cover the unchanged
Bybit code and shared runtime. No full-suite rerun is claimed for this additive
Coinbase/CI update. Incremental review is recorded in the merge commit.

## Linear order hooks and incremental integration 18261b604c

Two method rows advance independently of constructor completion:
requiresOrderLinkId returns false and validateOrderCapabilities adds no linear
restrictions. The fixture executes both actual TypeScript methods. Five successful
constructor cases check the false flag and four option shapes each, including
Plain zero amount, Bbo negative amount and TickSize. Rust exercises the concrete
Exchange trait route, which delegates to the source-mapped inherent method.
Separate compiling mutations (required IDs and BBO rejection) fail the source
state and empty-hook assertions. The BBO mutation was repeated after introducing
the inherent delegate, and the restored focused constructor test passes.

Host strict Clippy, separate wasm32 strict Clippy and formatting pass. Parity
passes at 7,334 rows / 3,199 public items. The inherited Coinbase separator-only
extractor drift clears after merging 18261b604c: zero manifest updates, additions
or orphans. Layer audit passes at 38 crates; wiring passes 251/251.

The already-started incremental merge retains all nine incoming paths exactly.
All-workflow named-step multisets are 350/350 -> 350, with no loss from either
parent. Cargo.lock, layer policy and Bybit Cargo.toml are unchanged. Real TOML
parsing gives 333 exemption blocks and 333 distinct targets with no blank
reasons. This is an incremental gate run, not a new full-suite result. The final
review verdict is recorded in the merge commit. Counts: 105 ported / 22 divergent
/ 393 pending, plus four journal pending. Generated IDs, placement composition
and the complete constructor remain unclaimed.

### Shared domain eligibility merge: 4041f6911c with e92bdd9b04

All 38 incoming paths retain the integration parent's exact content. No Bybit
production or manifest row changed. Both parents and the merged tree retain the
same 350 named CI steps. Cargo.lock, layers.json and Bybit's direct dependencies
are unchanged. Parsed exemptions are 338 blocks / 338 distinct targets, with
zero blank reasons and every target and reason from both parents retained.

All 65 scoped tests pass with live oracles enabled and zero ignored. Host strict
Clippy and separate wasm32 strict Clippy pass serially with one Cargo job. Layer
audit passes at 38 crates; parity passes 7,334 rows / 3,211 public items; extractor
reports zero updates, additions or orphans. Wiring files are unchanged from the
251/251 run in the preceding checkpoint. Counts remain 105/22/393 plus four
journal pending. Shared eligibility alone promotes no venue row. Final incremental
merged-state review is recorded in the merge commit.

## Inverse position fetch composition and refusal timing

The actual inverse mapper now evaluates logging, numeric coercion and the clock
before returning a closed-output refusal for inherited side or non-string symbol.
The composed fetch helper retains those outcomes through all rows and symbol
requests. Later source failures retain precedence, including the same original
HTTP error object on rejection. A source-successful operation refuses only at
its final typed boundary; it never publishes a partial result.

125 live-source composition cases execute actual fetchPositionsHelper,
reinitOnThrow and mapPosition with real p-iteration mapSeries. They assert exact
error kind/message, complete request traces/counts, logs, clock calls and reinit
state. All twelve inherited names are recorded by typeof and truthiness before
serialization. Seven non-string symbol shapes exercise the same ordering. The
existing single-row arithmetic oracle now pins the repaired side effects too.

Two plausible compiling mutations fail at exit 101: eager per-row refusal gives
RepresentationError instead of TypeError on constructor:later-null; eager scoped
refusal sends only one request instead of two on constructor:later-symbol-null.
Restoration passes all 52 library tests with live oracles enabled, zero ignored.
Host strict Clippy and separate wasm strict Clippy pass serially at jobs 1.
Parity passes 7,334 rows / 3,211 public items; extractor has zero drift.

fetchPositionsHelper advances to divergent; closed-output restrictions remain
explicit, while constructor and outer category fanout stay pending. Counts:
105 ported / 23 divergent / 392 pending, plus four journal pending. The final
adversarial review verdict is recorded with the commit.

## Concrete request reinitialization operation

The actual source reinitOnThrow field initializer now runs inside the constructor
fixture. Each successful constructor exercises success/rejection with both prior
flag states, retaining the exact original result object. The Rust concrete method
preserves the same Rc identity. A compiling success-clears-flag control fails the
named settled-state assertion when the flag starts true. Restoration passes.

Cancellation is explicitly CHOSEN: the source unawaited rejection tail still sets
the flag, whereas dropping the Rust wrapper unpolled or pending releases its
request and does not mutate the flag. Both drop shapes are measured and no owned
or detached task is created. The full constructor remains separately unclaimed.

Focused constructor test, formatting, host strict Clippy and separate wasm strict
Clippy pass; parity is 7,334 rows / 3,211 items; extractor has zero drift. Counts:
105 ported / 24 divergent / 391 pending plus four journal pending. Final review
verdict is recorded in the commit.

## Interface heritage census guard

Two interface-member walkers required guards: the recursive shape sampler and
the union oracle's explicit interface-field lookup. Both now refuse heritage
before enumerating own members. The numeric corpus inherits the shape guard
through --records; its separate placement scan already requires an explicit
TypeAlias with TypeLiteral, refusing an interface. Other .members reads extract
named class methods or enum values rather than claiming all interface fields.

Permanent inherited-required-field controls run in both repaired entrypoints.
Removing either guard makes its entrypoint exit 1 at the named missing-refusal
assertion. Restored validation passes 269 actual-source Rust shape round trips,
the union inventory and the 297-case numeric source corpus. No Rust production,
manifest claims or row counts change: 105/24/391 plus four journal pending.

## Resume merge: 5d263e0348 with a3d514bec8

All 77 incoming files exactly retain integration content. Both parents and the
merged tree retain 350 named CI steps. Cargo.lock, layers.json and Bybit's
Cargo.toml are unchanged. Real tomllib parsing reports 338 exemption blocks /
338 distinct targets with zero blank reasons.

All 66 scoped tests pass with live oracles enabled and zero ignored. Host strict
Clippy and separate wasm strict Clippy pass serially with jobs 1. Wiring passes
251/251; layer audit passes at 38 crates; parity passes 7,334 rows / 3,233 public
items; extractor reports zero updates, additions or orphans. Counts remain
105 ported / 24 divergent / 391 pending plus four journal pending. No row is
promoted by this merge. The merged-state review is recorded in the commit.

## Opposite-position checkpoint — 6d59250dfc

Built on inherited unreviewed WIP 52f664284e (parent e415a0bab8, integration
merge); no claim that this ancestry has been reviewed or merged. Initial diff
was genuine tests/boundary documentation/manifest work; no mutation was left.
The previous protective-side census remains a separate 54 Buy / 59 Sell /
85 None / 44 error claim; it was not rerun here.

Measured on clean implementation SHA 6d59250dfc:
- Scoped cargo check -p safe-cex-venue-bybit --all-targets --jobs 2: exit 0.
- Focused opposite_position_matches_source_signed_contracts live oracle: exit 0,
  1 passed, 0 ignored, 65 filtered out; enumeration stopped at case 395 of the
  fixed 396-case floor, with 347 false / 49 true outcomes.
- parity:extract:check via explicit Yarn JS: exit 0, output reports 7341 symbols,
  512 source files, zero would-update/add/orphan. Writer before commit also
  reported zero updates/additions/orphans, exit 0.
- Package fmt check and git diff --check before commit: exit 0.

Control on the same candidate production bytes: replacing signed >= with
abs(contracts) >= compiled and exited 101 at OPPOSITE native boolean 154
(true versus false). Production bytes were restored before 6d59250dfc; the
focused test passed afterward on that SHA. Native booleans are compared directly.
Read-only sub-agent review found no production parity defect; it ran only the
Node source corpus (exit 0), and noted that the separate boundary/surrogate
witness calls lack invocation counters. Those witness inventories are not a
proof against replacing their individual source calls with constants.

Not run: complete Bybit tests, workspace compilation, host or wasm Clippy,
parity Rust indexer, CI, or monorepo suites. Placement callers remain pending.

## Canonical Bybit journal — 94928cf7bb / metadata correction 61d0ebc159

Measured with Python 3.11 tomllib against integration 2bc83d78d4: owned Bybit
exchange plus six journal manifests move 0 pending->ported, 5 pending->divergent
(one opposite-position helper plus four journal rows). Baseline 108/29/407;
current 108/34/402. This includes the 24 journal rows in the pending denominator.
Parent actually built on: da43b67b89, with unreviewed inherited WIP 52f664284e
in its ancestry; no integration merge or PR performed.

On implementation 94928cf7bb, all-target Bybit tests with live oracles exited 0:
67 library + 9 live-data + 1 numeric-ingress + 2 ticker + 5 decode = 84 passed,
zero ignored, and the example targets ran zero tests. No test-source bytes
changed during this run. A manifest-only target-rename correction was committed
as 61d0ebc159 while its final targets completed; it changes no Rust/test code.

Before 94928cf7bb, six compiling production controls each exited 101 at a
named assertion: eager optional-leg evaluation (native success case 590),
finite-num-zero (native record count), closing-side inversion (native side),
funding-sign (native realizedPnl), partial-output (native record count), and
inherited fallback (native inherited refusal). Exact source restoration exited 0.
An initial typo in the eager mutation caused a compiler error, rejected by the
runner; only the corrected compiling mutation is counted as proof. Enumeration
stopped at case 594 of the 595-case source floor plus 20 numeric inputs:
29 source errors / 12 typed-output refusals / 554 ordinary successes.

Sub-agent review found and independently measured a survivor in the old plain
malformed-orderId witness: eager and lazy source paths both eventually threw
the same TypeError. The zero-size case now returns before key coercion; the
compiling Rust eager control fails that stronger witness. Leg-only/context-only
and direct fold_run boundary calls were added. Final sub-agent review on
94928cf7bb found no remaining production defect and ran the source oracle only.

On 61d0ebc159, scoped all-target cargo check --jobs 2 exited 0;
parity:extract:check exited 0 with 7341 symbols / 512 files / zero drift.
Writer after the rename also exited 0, zero updates/additions/orphans. Package
fmt and diff whitespace checks passed before the implementation commit.
Rust parity indexer exit 1 is NOT a pass: the first run caught our missing
opposite-position rename note; after correction no Bybit/journal diagnostic
remains, but inherited AsterDEX/WOO rename diagnostics and three core/AsterDEX
invention diagnostics remain. Their output was inspected, not suppressed.

Not run: workspace compilation, host/wasm Clippy, CI, or monorepo suites.
Typed callback throws and executable/getter inputs remain explicitly CHOSEN
boundary exclusions; no venue historical-position fetch composition is claimed.

## Sixteen pending helper rows — e20cc89ae0

Built on 42202de52a (Bitunix journal) after f51eeb7fe4 (BloFin journal),
c3b7acad5e (shared journal evidence), and inherited 52f664284e. The inherited
WIP remains an ancestor; no history was rewritten. Integration 4a7b848e03
already contains c3b7acad5e, verified by git merge-base --is-ancestor exit 0.

Python 3.11 tomllib census versus integration 4a7b848e03: the helper slice moves
3 pending->ported and 13 pending->divergent (16 total). Including the six venue
journal rows not yet integrated, delivery moves 3/19 (22 total). Owned Bybit+six
journal scope is 117 ported / 61 divergent / 366 pending. Bybit alone is
111 ported / 43 divergent / 366 pending. Earlier journal progress is not counted
again against this newer integration parent.

Measured on implementation e20cc89ae0:
- Scoped Bybit cargo check --all-targets --jobs 2: exit 0.
- Focused --test exchange_helpers with installed TypeScript: exit 0, 1 passed,
  zero ignored; each of eight exact-source restorations also exited 0.
- parity:extract:check via explicit Yarn JS: exit 0, 7341 symbols / 512 source
  files, zero would-update/add/orphan. Writer exited 0 before commit.
- Sub-agent review found no concrete defect and ran the source-only fixture
  pipeline at exit 0; it ran no Cargo, extractor or mutations.

One compiling mutation per new row family, each runtime exit 101 followed by
restored exit 0:
- String guards: HELPER native absent stringOrUndefined.
- Numeric guards: HELPER native absent parseBybitFiniteNumber.
- Mode predicate: HELPER native mode.
- Message predicates: HELPER native message predicate.
- Coin fold: HELPER native sumBybitCoinFields.
- Wallet balance: HELPER native deriveBybitAvailableBalanceFromCoins.
- First finite selection: HELPER native firstFiniteNumber.
- Inverse trigger selector: HELPER native inverse trigger direction.
No control survived. Scalars/optionals/number bits and decoded distinct ticker
axes are asserted directly; no native re-encoding comparison is used. Floors
stop at scalar 318/319, coin 15/16, first-number 5/6 and inverse-trigger 23/24,
with 48 independent selector comparisons. These are finite sampled floors,
not complete unknown-value or executable-object enumeration.

Venue journal evidence: BloFin control candidate f51eeb7fe4 compiled, failed
at JOURNAL native closedSize (1000x units mutation), then restored exit 0;
181 cases stopped at 180 plus six numeric inputs. Bitunix candidate 42202de52a
compiled, failed at JOURNAL native funding (sign mutation), then restored exit 0;
189 cases stopped at 188 plus six numeric inputs, including successful SELL.
Both focused adapter tests exited 0. Scoped all-target checks exited 0 on
42202de52a's committed venue code (BloFin and Bitunix serially). Sub-agent review
found no production defect; its missing SELL test finding was corrected.

Not run at this checkpoint: complete Bybit rerun, complete venue/core suites,
workspace compilation, host/wasm Clippy, or CI. Typed JSON/UTF-8 boundaries and
infallible venue callbacks remain explicitly CHOSEN; constructor and live venue
consumer composition are not claimed. Merge integration 4a7b848e03 DOWN before
push, then record the incremental merged-state gates separately.

## Integration-down checkpoint — 789b6a82935860a79f54914ca5b267a66494796a

Merged integration 4a7b848e03 into implementation/evidence parent 1ae62ec119.
Only conflict was competing Bitunix documentation additions; both retained.
Owned Bybit code, tests, mappings and journal manifests are identical to the
pre-merge implementation. Read-only sub-agent review found no concrete merge
issue; it ran no Cargo, extractor or mutations.

Measured at this merge SHA:
- Scoped Bybit cargo check --all-targets --jobs 2: exit 0.
- parity:extract:check through explicit Yarn JS: exit 0; output confirms 7341
  symbols / 512 source files and zero would-update/add/orphan.
- Focused exchange_helpers test with SAFE_CEX_TYPESCRIPT_PATH set: exit 0,
  1 passed / 0 failed / 0 ignored. An initial invocation without the required
  environment exited 101 at oracle startup (undefined require argument);
  this was an invocation error, not a passing gate or mutation control.
- git merge-base --is-ancestor 4a7b848e03 HEAD: exit 0.
- Python 3.11 tomllib independent census versus 4a7b848e03: 22 pending rows
  moved, 3 ported / 19 divergent; owned totals 117 / 61 / 366 respectively.

The eight compiling helper mutations and restored passes remain measured at
implementation e20cc89ae0, not relabeled as merged-state executions. BloFin
and Bitunix adapter gates retain their earlier named SHAs. No complete venue
suite, workspace build, host/wasm Clippy or CI was run at this checkpoint.
The documentation commit carrying this sheet changes no measured code.

## Eleven ledger/fee rows — 8563ce2c71ab6eea062877127d951fddc20b1302

Implementation parent: pushed 78f71e2e9b6b30ccd5d3fde2cfbce05e32d4b3af.
Python 3.11 tomllib census versus that parent moves 11 pending->divergent rows:
three order-history, three closed-PnL, three income and two fee-overlay rows.
All have canonical CHOSEN boundary notes and real native targets. Identical
source toNumber declarations share a native implementation. HTTP fetchers and
exchange consumer composition are still pending.

Measured on candidate 8563ce2c71:
- Nine compiling mutation controls each exit 101 at the named native assertion,
  followed by exact restoration and focused ledger_mapping test exit 0
  (1 passed / 0 failed / 0 ignored). The serial runner exits 0.
- toNumber sign: LEDGER native toNumber.
- Optional zero guard: LEDGER native absent toOptionalNumber.
- Positive timestamp guard: LEDGER native toTimestamp.
- Historical price/average-price swap: LEDGER native order price.
- Closed entry/exit-price swap: LEDGER native closed entryPrice.
- Category mapping: LEDGER native named category.
- Income fee sign: LEDGER native income amount.
- Fee-rate decoded field swap: LEDGER native fee taker input.
- Duplicate fee-symbol precedence: LEDGER native overlay takerFee.
- parity:extract:check via explicit Yarn JS: exit 0, 7341 symbols / 512 files,
  zero would-update/add/orphan. Writer previously exited 0, updated four
  manifests, added zero and orphaned zero.
No control survived. Every mapped native field is asserted directly before any
wire assertion; there is no native output re-encoding comparison. Enumeration
floors stop at scalar 101/102, order 24/25, closed row 23/24, income 22/23,
overlay 2/3 and category 8/9, plus one inherited-category witness. Record counts
come from independently executed maintained source, not the native output.

Sub-agent review found two eager representation-refusal defects before this
commit. Deferred historical enum refusals now allow later source coercion
errors and preserve the truthy inherited stop-type fallback short circuit.
Income category coercion still occurs before identity coercion, but category
representation refusal now occurs only for nonzero cashFlow. Empty and
funding/fee-only rows with inherited categories remain accepted. Five order
and three income witnesses were added. Re-review found no remaining defect;
source-only oracle exited 0. Its review ran no Cargo or mutations.

## Latest integration-down checkpoint — 4381c76cf5f86c403f70a9edd40c2f3950c036f6

Actual merge parents: candidate 8563ce2c71 and integration
ed8336d5835189fd470040fbe62756ff1b60a0ae. The fetch encountered a shared remote-ref
update race; the merge used the already advanced integration ref, confirmed
from the actual merge parents. No history or refs were force rewritten.
Owned ledger/helper code, tests, oracle and Bybit mappings retain candidate
content. Incoming Bybit ticker initialization includes its new timestamp field.

Measured on merge 4381c76cf5:
- parity:extract:check exit 0: 7341 symbols / 512 files, zero drift/add/orphan.
- Scoped Bybit all-target check exit 101, with E0046 (required monotonic_now_ms
  missing) and E0407 (old monotonic_ms no longer belongs to WsRuntime).
This failure is not counted as a passing gate. The sub-agent also found the
missing runtime method by inspection; its merge review ran no Cargo.

Correction eab00fbeb59f2b6b5da65dac5f6bf73fe96c07a8 moves the fixture's existing
self.monotonic.get() method from WsRuntime into ExchangeRuntime and renames it
monotonic_now_ms. No production behavior or manifest row changed. Read-only
sub-agent review found no issue in the method move and ran no Cargo.

Python 3.11 tomllib census on merged owned scope versus either integration
4a7b848e03 or ed8336d583: 33 pending rows moved, 3 ported / 30 divergent.
Owned Bybit+six journal totals: 117 ported / 72 divergent / 355 pending.
Bybit alone: 111 ported / 54 divergent / 355 pending. The second slice adds
11 rows to the earlier pushed 22-row delivery, not a repeated count of earlier
journal work already present in integration.

Measured on corrected code eab00fbeb59f2b6b5da65dac5f6bf73fe96c07a8:
- Scoped cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit
  --all-targets --jobs 2: exit 0, zero compile errors.
- cargo test with --test exchange_helpers --test ledger_mapping --jobs 2
  -- --include-ignored and installed TypeScript path: exit 0, one passed /
  zero failed / zero ignored in each focused binary (two total).
- Existing --lib ws_public::tests filter --jobs 2: exit 0, 13 passed /
  zero failed / 17 ignored / 37 filtered out. The ignored source websocket
  tests were NOT run at this checkpoint; this is not a complete venue suite.
- git merge-base --is-ancestor ed8336d583 HEAD: exit 0.
- git diff --check: exit 0.

The evidence-only commit carrying these results changes no measured code.
Mutation exits remain pinned to candidate 8563ce2c71, extractor exits to
8563ce2c71 and merge 4381c76cf5; they were not re-executed or relabeled at the
fixture compatibility commit. Not run: workspace compilation, complete venue
or core suites, host/wasm Clippy, or CI. Inherited WIP 52f664284e remains an
ancestor as already reported; this sheet does not claim a full ancestor audit.

## Reopened PR 7454 recovery — integration f436fc8163

PR 7450 integrated the earlier 22-row delivery. Independent Python 3.11 tomllib
census confirms the later eleven ledger/fee rows were still pending at
f436fc816390a8b4556ab7a2fe0ae85072337036. Relative to that parent this branch
moves 0 pending->ported / 11 pending->divergent, with Bybit totals
111 ported / 54 divergent / 355 pending. Earlier 22 rows are not counted again.

At the user's instruction, resumed feat/bybit-opposite-position for the
reopened PR rather than publishing a duplicate recovery branch. Merge
c1467d3f539b9a3df543a3adcded19e3638337fe has parents d25fe0e0e9 and f436fc8163.
Then fetched integration and merged e150dc59a2d5db2af0ff1875c92f291f2f32b9f4,
producing 653f6b75b50d799f8ab3ce01e3ffeea37c384c3a. Both merges were clean.
No PR was opened or merged by this agent. Future slices use fresh branches.

Measured on c1467d3f53: focused exchange_helpers and ledger_mapping tests with
installed TypeScript and --include-ignored exit 0, one passed / zero failed /
zero ignored each. Extractor writer exit 0, zero updates/additions/orphans.
Measured on 653f6b75b5: scoped Bybit cargo check --all-targets --jobs 2 exit 0;
parity:extract:check exit 0, 7341 symbols / 512 files and zero drift/add/orphan.

Byte-retention proof: git diff --exit-code c1467d3f53 HEAD -- Bybit src/tests
exits 0 after the integration merge. Build dependencies, Cargo manifests/lock,
maintained Bybit TypeScript and shared types also match the separately checked
741d337f8d recovery tree (diff exit 0). That recovery tree was reviewed by a
sub-agent, no concrete defect found; it ran no Cargo/extractor/mutations.
The nine compiling ledger controls remain measured at 8563ce2c71 with named
assertion exit 101 and exact-restoration exit 0. Their code/tests/oracle/scripts
are byte-identical to this recovery; controls were not rerun or relabeled.

Not run during this recovery: complete venue/core suites, websocket test rerun,
workspace compilation, host/wasm Clippy, or CI. This evidence-only update changes
no measured source or tests; final fetch/merge retention is checked before push.

Final fetched integration was 4e0ca1accd6fc8a81fb6d163581861c9860f8da9,
merged as 6c57810483e5203ef9d80a52f6a08a9c38ea9b77. Bybit src/tests retention
versus gate SHA 653f6b75b5 exits 0. The wider Cargo.lock comparison exited 1
because Coinbase gained an indexmap dependency; no Bybit code changed.
Scoped Bybit --all-targets --jobs 2 check was rerun on 6c57810483 and exited 0.
Extractor check was also rerun on 6c57810483: exit 0, 7341 symbols / 512 files,
zero drift/add/orphan. The two source-oracle test exits remain measured at
c1467d3f53, with unchanged Bybit source/tests and shared source dependencies;
they were not relabeled as fresh executions at 6c57810483.

## Variant accessor slice: original parent 2c17c1976a

Fresh branch feat/bybit-variant-accessors starts directly at integration 2c17c1976a; no unreviewed diagnostic-slice ancestor. Implementation 04f4dcde890e546df18b04d127ab9e15a959dc80; target-header-only fix d398c242a3e1c86aeecd8a9f8ca2a68101edce83. Python 3.11 tomllib census at d398c242a3 versus parent exits 0: exactly 12 moved (6 pending->ported, 6 pending->CHOSEN divergent), Bybit 117 ported / 60 divergent / 343 pending. Classes, constructors, lifecycle consumers and other variant methods stay pending.

Measured scoped cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2 at 04f4dcde89: exit 0. Only manifest metadata changed in d398c242a3. Extractor writer after deriving all three target-file arrays from mapped symbols: exit 0, zero added/orphaned; final header writer updates one file. Extractor check at d398c242a3: exit 0, 7341 symbols / 512 files, zero drift. Direct file-header target coverage assertions: exit 0. No standalone Rust indexer gate was run.

At d398c242a3, scripts/variant-accessor-controls.py exits 0: initial live-native baseline exits 0 (1 passed / 0 failed / 0 ignored), seven compiling mutations each exit 101 at the named native assertion, and all seven exact restores exit 0 (1 passed / 0 failed / 0 ignored). Tracked evidence/variant-accessor-controls.json binds each measured exit and named assertion to d398c242a3. Whole production/test/script retention diff after controls exits 0. Seven controls are the selected family floor, stopped after settlement logging (iteration 7); zero survivors.

All comparisons assert decoded native values and captured host-call arguments; there is no native encode or roundtrip gate. Live maintained-source floors stop at account 38/38 (index 37), category 12/12 (11), ticker 40/40 (39), settlement 104/104 (103), numeric 29/29 (28); these are bounded input floors, not exhaustive input proofs. Exact source members and inherited base subscription bodies are extracted by AST; unique-member assertions protect the oracle. Tests retain duplicate symbol order, [undefined] settlement, warning/log calls and host error name/message/metadata propagation.

Read-only sub-agent review and final manifest/control review found no defects; source-only oracle exit 0 was independently measured by the reviewer. Reviewer ran no Cargo or controls. A first development oracle invocation exited 1 because three class extracts redeclared Subject in one VM; block scoping fixed the harness before the committed baseline. This was not a mutation control.

No full venue suite, workspace build, monorepo suite, host/wasm clippy or CI rerun. Six CHOSEN notes bound typed Market/UTF-8/inert JSON and explicit host callback differences; these methods do not claim constructor wiring.

## Eleven failure-diagnostic rows — da0ab97df12b7dd87102d8beffe061d93c1e707e

Fresh branch feat/bybit-failure-diagnostics starts directly at integration
61c78429c3; no preceding unmerged slice is its parent. Python 3.11 tomllib
census versus that parent moves 1 pending->ported / 10 pending->divergent
(11 total), leaving Bybit 112 ported / 64 divergent / 344 pending. This is a
new slice, not another commit series on the previously merged branch.

Mapped families: exact cooldown constant; create-order response declaration;
telemetry and reason description; retry classifier; error-field precedence;
market-request error/class constructor; leverage predicate; Hyro rejection
error and duplicate predicate. Anonymous telemetry return fields have a C3
representation exemption only; no extra behavior or invented source row.

On candidate da0ab97df1, nine compiling controls each exit 101 at a named
native assertion, followed by exact source restoration and focused test exit 0
(1 passed / 0 failed / 0 ignored). Serial runner exit 0, no survivors:
- Constant: DIAGNOSTIC native cooldown.
- Symmetric terminal/recorded ingress+egress swap: DIAGNOSTIC native response terminal.
- Telemetry precedence: DIAGNOSTIC native telemetry reason.
- Retry range: DIAGNOSTIC native retry.
- Non-nullish field guard: DIAGNOSTIC native error field.
- Error metadata: DIAGNOSTIC native request tags.
- Leverage code: DIAGNOSTIC native leverage.
- Rejected-error identity: DIAGNOSTIC native rejected name.
- Duplicate predicate: DIAGNOSTIC native duplicate.
parity:extract:check at da0ab97df1 exits 0, 7341 symbols / 512 source files,
zero would-update/add/orphan. Writer previously exits 0, updates two manifests,
adds zero and orphans zero. Native fields and numeric bits are asserted directly;
there is no native decode/re-encode wire comparison.

Correction 705fa5ab7996d66111dfe507bf87cbc45b9c3917 preserves source constructor
omission as Option<f64>, defaulting None to positive zero after telemetry.
Seven independent source counter cases cover undefined, signed/positive zero,
finite fractions/negative values and Infinity/NaN. The default-only compiling
control exits 101 at DIAGNOSTIC native request default tags, exact restoration
exits 0 (1 passed / 0 failed / 0 ignored), runner exit 0. Earlier nine controls
remain measured at da0ab97df1 and are not relabeled as executions on 705fa5ab79.
Scoped cargo check -p safe-cex-venue-bybit --all-targets --jobs 2 on 705fa5ab79
exits 0, zero compile errors. Sub-agent re-review finds no concrete defect;
source-only oracle exits 0; no Cargo or mutations were run by the reviewer.

Floors stop at failure 35/36, field 19/20, leverage 11/12, duplicate 14/15,
rejection 2/3, response 13/14 and constructor-counter 6/7. The response's exact
maintained seven-field declaration is independently TypeScript-typechecked:
4 native accepts / 8 compiler refusals / 2 source-accepted array-record native
refusals. Own prototype values and nonfinite retry metadata have explicit source
success/native refusal witnesses and CHOSEN notes. These are sampled floors;
no executable-object, custom-prototype or exhaustive unknown-value proof is claimed.

An initial fixture invocation failed at JSON decoding because three control
characters were embedded in raw JSON strings. It was corrected before the
candidate; fixture JSON parsing now occurs outside source-exception capture,
so syntax errors cannot stand in for predicate behavior. A failed manifest
script parsed no changes; the corrected script and writer produced the two
canonical manifests. Neither mistake is counted as a mutation gate.
The cached Rust indexer attempt exits 1 before validation at
safe-cex-webworker/src/twap_manager.rs's unsupported thread_local macro;
this is not a Rust-indexer pass and does not validate new targets.

Not run: workspace build, complete venue/core suites, host/wasm Clippy, or CI.
HTTP startup, order submission and recovery composition remain pending.

## Diagnostic slice integration-down — f76b90a1114789886796a2b990eb2dd31d6d629e

Actual merge parents: evidence 702ae5520c and fetched integration
cd7371a9b93dd166cfe6fcb5163f6a5439640dc5. Bybit src/tests/scripts and owned
manifests match 705fa5ab79 exactly, git diff --exit-code exits 0. The C3 telemetry
exemption survived. Read-only sub-agent merge review found no concrete issue;
it ran no Cargo/extractor/mutations. Shared crypto/num/store/exchange sources
and Cargo.lock changed, so scoped gates were rerun rather than treating the
pre-merge dependency tree as the merged build.

Measured at f76b90a111:
- Scoped Bybit cargo check --all-targets --jobs 2: exit 0, zero compile errors.
- Focused --test failure_diagnostics --jobs 2 -- --include-ignored with
  SAFE_CEX_TYPESCRIPT_PATH set: exit 0, 1 passed / 0 failed / 0 ignored.
- Explicit Yarn JS parity:extract:check: exit 0, 7341 symbols / 512 files,
  zero would-update/add/orphan.
- Python 3.11 tomllib census versus 61c78429c3: exit 0, 1 pending->ported /
  10 pending->divergent (11 rows); Bybit 112 / 64 / 344 respectively.
- git diff --check: exit 0.

Mutation exits remain pinned to da0ab97df1 and 705fa5ab79 as named above;
none were relabeled as merged-state executions. Complete venue/core suites,
workspace compilation, host/wasm Clippy and CI were not run. The evidence-only
commit changes no measured source/tests. Final integration fetch and retention
proof are performed immediately before pushing this fresh slice branch.

Final fetch advanced integration to 2c04dafa36dd53b183f10b974fcd0a4adf2ada1a,
merged as b45fc9949fdd231ec2eeadd32aeea476ba467553. Bybit src/tests/scripts
remain byte-identical to f76b90a111 (retention diff exit 0). The broader
shared-source comparison exits 1 solely for an added, unused
BaseWebSocket::has_websocket method; the attempted push stopped before sending.
Scoped Bybit all-target check and extractor check were rerun on b45fc9949f:
both exit 0, extractor 7341 symbols / 512 files with zero drift/add/orphan.
The diagnostic oracle remains measured at f76b90a111; its code and relevant
coercion/metadata inputs remain unchanged and its exit was not relabeled.
Manifest-only notes now explicitly state that synchronous source coercion throws
adapt to message-only Result errors, excluding thrown-error name/stack/prototype
identity. Native request/rejection error objects still retain their successful
construction names and rendering, as directly asserted. Writer after those
notes exits 0, updates two manifests, zero additions/orphans. No code or row
status changed in this note update; counts remain 1 ported / 10 divergent moved.

Latest integration 0f02e1219579430184111306d15e12e7f0864141 was merged as
a0fbc7a97118477f79b018f779f5a93dbdc50b05, parents 4c1b797570 and 0f02e12195.
Bybit src/tests/scripts remained unchanged (quiet retention diff exit 0).
Shared metadata/token modules were added, so scoped check and extractor were
rerun on a0fbc7a971: both exit 0, zero compile errors, 7341 symbols / 512 files
and zero extractor drift/add/orphan. The prior oracle and mutation exits retain
their named implementation SHAs; no full workspace/venue/Clippy/CI claim is made.
Extractor check after the error-result note-only commit 4c1b797570 also exits 0.

Final merge 7a3529326b2bb7d3fc0767f3e5c53ef68d818bd0 retains incoming integration
9c151409d125381f67d08d2b644afa89c77a79c8. Competing exemption appends were
resolved by retaining both complete Bybit and ApexOmni entries; TOML parsing
and independent singleton checks for both targets passed. Retention versus
scoped-gate a0fbc7a971 exits 0 for Bybit src/tests/scripts and all shared source
dependencies. No scoped build or oracle was rerun for this unchanged-input merge.
Extractor check on 7a3529326b exits 0, 7341 symbols / 512 files, zero drift.
Tomllib census on that merge versus 61c78429c3 exits 0: 1 ported / 10 divergent
moved, totals 112 / 64 / 344. The whole incoming staged whitespace check exited
2 for six MEXC evidence-log blank EOF lines; those raw logs were preserved and
this is not counted as a clean global whitespace gate. Owned changes are clean.

## PR 7489 merge-down: 04d63197b4ebb01e29035299b045135fa9051aa2

Merged integration 2c17c1976a. Both competing Coinbase and Bybit exemptions retained; Python 3.11 tomllib parsing and singleton assertions exit 0. The default Python lacked tomllib (exit 1); the shell continued and merge 2159855e9c temporarily committed conflict markers, corrected without rewriting history by 04d63197b4. No broken revision was pushed.

At 04d63197b4: extractor check exit 0, 7341 symbols / 512 files, zero updates/additions/orphans. Own src/tests/scripts retention diff against gate SHA a0fbc7a971 exit 0: previous measured scoped compilation and native oracle/control exits remain attached to their original SHAs above; neither build nor oracle was rerun for this conflict-only merge. Python 3.11 tomllib census against actual slice parent 61c78429c3 exit 0: 1 pending->ported, 10 pending->divergent; Bybit 112 ported / 64 divergent / 344 pending. No unreviewed slice ancestor at original branch creation; integration was merged down. No workspace build, full venue suite, host/wasm clippy or CI rerun.

## Variant slice post-merge gates — dacf680310ac9058a8f663fdca5b6e2dfebee521

Actual merge parent is integration 3a4f19c35ab9a8a795bef393a969448fc2cad908. Its PR 7489 diagnostics are now reviewed integration work. Both competing module registrations and both gate-sheet sections were retained. Whole Bybit source/test scope differs from d398c242a3 because the diagnostic module/tests were imported: the changed-input case applies. Scoped cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2 was rerun at dacf680310: exit 0. Focused native oracle cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test variant_accessors --jobs 2 -- --include-ignored (SAFE_CEX_TYPESCRIPT_PATH set to the installed TypeScript) was rerun at dacf680310: exit 0, 1 passed / 0 failed / 0 ignored. Extractor check at dacf680310: exit 0, 7341 symbols / 512 files, zero drift/additions/orphans.

Own accessor implementation/test/oracle/runner retention diff d398c242a3 -> dacf680310: exit 0. Shared number/type/exchange/store source and all source venue/base oracle inputs retention diff: exit 0. Seven mutation/restoration exits therefore remain measured at d398c242a3; controls were not rerun after this merge. Sub-agent merge review independently confirmed byte retention and all 12 mappings; no findings. Python 3.11 tomllib census at dacf680310 versus its actual merged integration parent exits 0: 6 pending->ported, 6 pending->divergent, 12 total; Bybit 118 ported / 70 divergent / 332 pending. The imported 11 diagnostic rows are excluded from this slice's delta. Tracked evidence/variant-accessor-census.json contains both actual SHAs, totals and individual moved rows. Original branch parent remains 2c17c1976a. No additional full-suite/workspace/clippy/CI claims.

## Final variant merge retention — 5111a4891f849147c89f618b9571c09c2b9ed8c0

Immediately before delivery fetched integration and pinned FETCH_HEAD a7e905dac468e2ce4ba98bbeee5b86b3f8a4fd87, then merged it down at 5111a4891f. Whole Bybit src/tests/scripts diff --exit-code dacf680310 HEAD exits 0: byte-unchanged case applies. Shared num/types/exchange/store/crypto/transport source and live oracle venue/base source retention diff exits 0. Scoped check and native oracle remain measured at dacf680310 (both exit 0), not relabeled as fresh runs at 5111a4891f; seven compiling controls remain measured at d398c242a3. Final extractor check freshly run at 5111a4891f exits 0: 7341 symbols / 512 files, zero drift/additions/orphans. Owned patch whitespace check against original parent exits 0.

Final Python 3.11 tomllib census at 5111a4891f versus pinned merged integration a7e905dac4 exits 0: exactly 12 new moved rows, 6 pending->ported / 6 pending->CHOSEN divergent. Integration 112 ported / 64 divergent / 344 pending; branch 118 ported / 70 divergent / 332 pending. Updated tracked census contains these exact measured SHAs. The original parent is 2c17c1976a; no unreviewed slice was used as the branch base. Evidence-only delivery commit retains this measured revision's production/tests byte-for-byte.

## Inverse/spot recent-order cache — 833ebb09cf36f2244f2a6a73ebf8035aeac009fb

Historical evidence retired: the standalone cache facade, its fake-host oracle, mutation runner and generated control files have been removed. Production cache rows now map to the native lifecycle receiver; variant_reconciliation_binding, private_frame_binding, inverse_batch_binding and Brazil Spot cancellation cover the live consumers. The commands and counts below describe the removed facade only.

Fresh branch feat/bybit-local-order-cache starts directly at reviewed integration cc3d25150b; no unreviewed slice ancestor. Initial implementation dda677324a, callback correction bf6f6cc1e9, control-only correction 833ebb09cf. Python 3.11 tomllib census at 833ebb09cf against original parent exits 0: exactly 10 pending rows moved (2 pending->ported / 8 pending->CHOSEN divergent), Bybit 120 ported / 78 divergent / 322 pending. Five identical source families across inverse and spot map to five real Rust functions. Constructors, REST consumers and the linear exchange's distinct timer/reconciliation cache remain pending. Two C3 exemptions name anonymous Map storage and its read-only observer, not fabricated exchange implementations. Target-file arrays were mechanically derived from mapped rows and serialized by extractor writer.

Measured at 833ebb09cf: scoped cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2 exits 0. Earlier scoped check dda677324a also exited 0, but its later-changed implementation does not cover this revision. scripts/local-order-cache-controls.py at 833ebb09cf exits 0: live baseline exits 0 (1 passed / 0 failed / 0 ignored), all eight compiling mutations exit 101 with the recorded named native assertion, and all eight restores exit 0 (1 passed / 0 failed / 0 ignored). Tracked evidence/local-order-cache-controls.json binds every exit to this exact SHA. Exact src/tests/scripts restoration diff exits 0. No runtime mutation survivors. Selected control floor is eight (five new families and three actual review regressions); stopped after read-only store reentry, iteration 8.

The first control attempt at bf6f6cc1e9 exits 1: three valid named runtime controls were caught/restored; fourth cancellation mutation exits 101 at compilation because replacing id entirely caused an unused-variable error under deny warnings. That is an invalid control, not runtime detection or a survivor; it restored with test exit 0. Tracked evidence/local-order-cache-controls-invalid.json preserves its results. The corrected cancellation mutation uses id and now compiles, fails CACHE native cache count, restores, and passes in the final run. No failed attempt is counted as a passing gate.

Extractor writer exits 0 after manifest edits (two file updates, no added/orphaned symbols; note-only subsequent writer zero updates). Extractor check at bf6f6cc1e9 exits 0: 7341 symbols / 512 files, zero drift/additions/orphans. Only the control runner/evidence changed afterward. tomllib file-header coverage and two singleton C3 assertions at bf6f6cc1e9 exit 0. Owned patch whitespace check against cc3d25150b exits 0 at 833ebb09cf. No standalone Rust indexer gate was run.

Read-only sub-agent review found three concrete callback gaps: early site capture, early store-list capture, and holding a cache borrow across read-only store inspection. Source-only witnesses exited 0; all three were corrected before final gates, and each has a named discriminating regression control. Final source-only reviewer oracle exits 0 (30 scenarios / 98 steps); final review reports no unresolved findings. Reviewer ran no Cargo or controls. Ordinary Store reads may inspect cache, clock callbacks may update site/store lists, store notifications may mark an order seen or fail after insertion/deletion. All native object references and mutable Order fields are asserted directly; key evaluation before clock, insertion order, first store match, strict 8000ms expiry, NaN/infinite/signed-zero clocks and early guards are covered. No encode or roundtrip assertions are used.

Bounded maintained-source floors: 22 fallback cases, stopped at index 21; 15 programs per variant, 30 scenarios total, stopped at scenario index 29; 49 steps per variant, 98 observations total, stopped at observation index 97. These are selected floors, not exhaustive input proofs. CHOSEN notes explicitly exclude arbitrary executable/getter rows, lone UTF-16 surrogates, cache-mutating Store getters, clock edits to fetched-array membership, and JS array/error object identity outside the native ABI. Mutable order identity is retained by Rc/RefCell. No full venue/monorepo/workspace suites, host/wasm clippy, or CI rerun.

## Cache post-merge gates — d48fa375f612b5fa94c2f81791bfe060590224e3

Merged reviewed integration 852b13d6c0385f0f1ae607af880953c335f61726. Sole conflict was competing exemption appends; both Bybit C3 entries and all incoming entries retained, tomllib parse and singleton assertions exit 0. Whole Bybit src/tests/scripts retention diff against 833ebb09cf exits 0: byte-unchanged case applies. Relevant shared types/num/ExchangeError and maintained-source oracle input retention diff exits 0. Eight controls remain measured at 833ebb09cf; they were not relabeled or rerun for this merge.

Shared crypto/store/exchange/transport source and lockfile changed, so at d48fa375f6 reran scoped cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2: exit 0. Focused cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test local_order_cache --jobs 2 -- --include-ignored with installed TypeScript env: exit 0, 1 passed / 0 failed / 0 ignored (30 scenarios / 98 source observations). Extractor check at d48fa375f6 exits 0: 7341 symbols / 512 files, zero drift/additions/orphans. Read-only sub-agent merge review found no loss or new issue; reviewer ran no Cargo or controls.

Python 3.11 tomllib census at d48fa375f6 versus actual merged integration parent 852b13d6c0 exits 0: exactly ten moved rows (2 pending->ported / 8 pending->CHOSEN divergent), Bybit 120 ported / 78 divergent / 322 pending. Tracked census updated with these actual SHAs. Original branch parent is cc3d25150b. No additional workspace/full-suite/clippy/CI claims.

## Final cache merge retention — c7fae97894b133b474ae34841b1620d243f5bca4

Final pre-push fetch pinned FETCH_HEAD 6e5c3103e731ff6cb5c7198861d53ff0b70514c2, merged down at c7fae97894. Whole Bybit src/tests/scripts retention diff against gate SHA d48fa375f6 exits 0: byte-unchanged case applies. Shared num/types/exchange/store/crypto/transport crates, workspace Cargo.toml/Cargo.lock, and live oracle source inputs retention diff also exits 0. Therefore scoped check/native test exits remain measured at d48fa375f6 (both 0), eight named runtime mutation/restoration exits at 833ebb09cf (101/0 each); no relabeling or redundant Cargo rerun. Fresh extractor check at c7fae97894 exits 0: 7341 symbols / 512 files, zero drift/additions/orphans.

Final Python 3.11 tomllib census at c7fae97894 versus actual final merged integration 6e5c3103e7 exits 0: exactly ten new rows, 2 pending->ported / 8 pending->CHOSEN divergent. Integration 118 ported / 70 divergent / 332 pending; branch 120 ported / 78 divergent / 322 pending. Tracked census contains both complete measured SHAs and individual rows. Original branch parent remains cc3d25150b, with no unreviewed slice ancestor at creation. Evidence-only delivery commit retains measured production/test/script bytes. No new suite/workspace/clippy/CI claims.

## Execution guards — 62d5514d2f11b5ad724f42c84e73b7dd777ed9e1

Fresh branch feat/bybit-execution-guards starts directly at reviewed integration 5775b542a5; no preceding unreviewed slice is an ancestor at creation. Python 3.11 tomllib census at 62d5514d2f versus that parent exits 0: 12 pending rows moved (1 pending->ported / 11 pending->CHOSEN divergent), Bybit 121 ported / 89 divergent / 310 pending. Seven real Rust functions map inverse windows, inverse/spot reinit-on-throw, three execution wrappers and six position-index methods. No extra public support types or exemptions were introduced. Constructors, position-mode transport and order consumer composition remain pending.

At 62d5514d2f: scoped cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2 exits 0. scripts/execution-guard-controls.py exits 0: initial live-native baseline exits 0 (1 passed / 0 failed / 0 ignored), seven compiling mutations each exit 101 at the named assertion, and all seven exact restores exit 0 (1 passed / 0 failed / 0 ignored). Tracked evidence/execution-guard-controls.json binds every exit/assertion to this measured SHA. Whole src/tests/scripts restoration diff exits 0. No survivors or invalid controls in this run. Selected new-function control floor is seven, stopped at stop omit forwarding (iteration 7).

Extractor writer after deriving file arrays from actual targets exits 0: updates three manifests, no added/orphaned symbols; boundary-note-only writer exits 0 with zero updates. Extractor check at 62d5514d2f exits 0: 7341 symbols / 512 files, zero drift/additions/orphans. Python tomllib file-header coverage assertions and owned patch whitespace check against original parent exit 0. No standalone Rust indexer gate was run.

Bounded maintained-source oracle floors: 20 window cases (stopped at index 19), 8 rejection cases (7), 87 position-index cases (86), 24 stop-index cases (23), 39 execution-wrapper cases (38). Exact source class members, real enum/validator declarations and installed actual Lodash omit are evaluated, with unique-member checks. Native values, IEEE bits, option-map arguments, four decoded execution output fields and Rc error/value identity are asserted before callback-trace comparisons. Controlled futures cover pending/ready/rejection phases and option mutation during await. Nonterminating/stagnating window ranges and full input-space enumeration were not tested; these floors do not claim completeness. There are no native output encode/roundtrip gates.

Read-only sub-agent source oracle exits 0 on these floors. Review found a documentation boundary gap for malformed non-string symbols: owned async JSON snapshots retain values but exclude nested JS aliases. Both position-index notes now state that restriction. Final sub-agent review finds no remaining issue on declared paths, verifies all twelve real mappings and counts (tomllib exit 0), and confirms runner restoration/detection. Reviewer ran no Cargo or controls. CHOSEN notes also state Rust Future/poll scheduling versus JS Promise identity/microtasks, already-decoded immutable execution options, typed Rc ExchangeError boundaries, and nested alias/getter restrictions for omitted option copies.

Before the committed baseline, the development source oracle first exited 1 because local worktree Lodash was absent; using installed dependency resolution fixed it. A development Cargo test then exited 101 for a wrong numeric helper namespace; numeric_ingress qualification fixed it before the measured commit. Neither development failure is counted as a mutation control or passing gate. No full venue/monorepo/workspace suites, host/wasm clippy or CI reruns.

## Execution guard post-merge gates — 08e5a2dfb9bf7a82b707a471717b725c9a54bfb6

Merged reviewed integration 972c5ec9bd8c4bb2023530199c45961dfca86229 without conflicts. Whole Bybit src/tests/scripts retention diff against control SHA 62d5514d2f exits 0: byte-unchanged case applies. Shared build-input retention comparison exits 1; independent changed-path enumeration shows Cargo.lock only. Accordingly, at 08e5a2dfb9 reran scoped cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2: exit 0. Reran focused cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test execution_guards --jobs 2 -- --include-ignored with installed TypeScript env: exit 0, 1 passed / 0 failed / 0 ignored. Fresh extractor check: exit 0, 7341 symbols / 512 files, zero drift/additions/orphans. Seven control exits remain measured at 62d5514d2f, not relabeled as fresh post-merge controls.

Python 3.11 tomllib census at 08e5a2dfb9 against actual merged integration 972c5ec9bd exits 0: exactly twelve new rows, 1 pending->ported / 11 pending->CHOSEN divergent. Bybit 121 ported / 89 divergent / 310 pending. Tracked census updated with these full measured SHAs and individual moved rows. Read-only sub-agent merge review confirms all twelve mappings, notes and headers retained, no new issue; reviewer ran no Cargo/extractor/controls. Original branch parent remains 5775b542a5. No new workspace/full-suite/clippy/CI claims.

## Final execution guard retention — bb1b2d1c92784626bf174a32cba1906b6863c61f

Final pre-push fetch pinned integration 972c5ec9bd8c4bb2023530199c45961dfca86229; merge reports already up to date. Whole Bybit src/tests/scripts and all Bybit manifests diff --exit-code 08e5a2dfb9 HEAD exits 0: byte-unchanged case applies. The scoped check, focused native test and extractor exits remain measured at 08e5a2dfb9 (all 0); seven named mutation/restoration exits remain measured at 62d5514d2f (101/0 each). No additional Cargo/extractor run is claimed for this evidence-only revision. Census stays bound to actual measured head 08e5a2dfb9 and actual merged integration 972c5ec9bd; manifest retention proves the same 1 pending->ported / 11 pending->CHOSEN divergent delta, twelve total, 310 pending, at delivery. Original parent 5775b542a5 had no unreviewed slice ancestor. Delivery evidence-only commit preserves these measured code, test, script and manifest bytes. No new workspace/full-suite/clippy/CI claims.

## Account readers — e020e56507fe3f6205e2cff9ab60994ac6dc92c7

Fresh branch `feat/bybit-account-readers` was created directly from reviewed integration 518d08ff3e; no unreviewed slice was an ancestor at creation. Python 3.11 tomllib census versus that parent exits 0: exactly 10 pending rows moved, 0 pending->ported / 10 pending->CHOSEN divergent. Bybit changes from 121 ported / 89 divergent / 310 pending to 121 / 99 / 300. Tracked evidence/account-reader-census.json enumerates the ten independently selected symbols and their real native targets. All 26 Bybit manifests were enumerated, stopping at syncUnifiedBalanceUpnlFromPositions.ts.toml. This is a census, not proof of remaining runtime behavior.

Five native functions map primary/inverse/spot account identity, margin metadata and balance readers, plus HyroTrader balance-to-store update. No class/constructor/composition rows are claimed and no public support types or C3 exemptions were added. Canonical CHOSEN notes describe owned plain JSON and tuple boundaries, executable/getter/prototype and alias exclusions, explicit transport/state/store callbacks, retained error identities without JS stack/prototype identity, and Future polling versus Promise scheduling. Hyro's native update callback takes the retained balance directly; assembling the anonymous store payload is a host responsibility. The pre-existing account-type note now points to the separately mapped margin reader rather than calling it pending.

At e020e56507: `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2` exits 0, zero compile errors. Extractor check at that SHA exits 0: 7341 symbols / 512 source files, zero drift/additions/orphans. The writer after manifest edits exits 0 (four canonicalized manifests); writer after the pending-note correction exits 0 (zero updates). Four file-header coverage assertions exit 0, with arrays mechanically derived from real row targets. Owned patch whitespace check versus the parent exits 0. The initial pre-commit development focused test exited 0; it is not relabeled as a measured committed gate.

At d5048540f5c566b43214fc5c65826d2c36b09ce0: `python3.11 sdk-rs/safe-cex-venue-bybit/scripts/account-reader-controls.py` exits 0. Its baseline focused test exits 0 (1 passed / 0 failed / 0 ignored). Five compiling mutations each exit 101 at a named runtime assertion; every exact restore exits 0 (1 passed / 0 failed / 0 ignored). Tracked evidence/account-reader-controls.json binds all measured exits and names to that SHA. Selected control floor is five, stopped at Hyro store update guard (iteration 5); zero survivors and zero invalid controls. The runner's copied floor metadata was corrected from seven to five before this measured run. Whole src/tests/scripts restoration diff against d5048540f5 exits 0. Only the runner's evidence metadata differs from the original scoped-check revision; production and tests are unchanged.

| New native family | Deliberate mutation | Named failing assertion | Mutation / restore |
| --- | --- | --- | --- |
| Account identity | Read inviterID for userId | READER native scalar userId | 101 / 0 |
| Linear/inverse margin | Select Cross for isolated mode | READER native margin type | 101 / 0 |
| Spot margin | Read marginMode for unifiedMarginStatus | READER native scalar unifiedMarginStatus | 101 / 0 |
| Balance reader | Invert disposed guard | READER native stored balance identity | 101 / 0 |
| Hyro margin/store | Invert active guard | READER native present updated balance | 101 / 0 |

Bounded live-source floors are 36 account cases (stopped at index 35), 36 margin cases (35), 24 balance cases (23), and 8 Hyro cases (7). Actual maintained AST class members, endpoint declaration and MarginType enum are uniquely selected and evaluated. Tests assert decoded native userId/affiliateId independently, missing versus null, strict status decisions, native enum/status values, partial state changes, post-await disposal/store reads, retained Rc balance/error identity and formatter/update failures before callback traces. Native fields and IEEE bits are compared directly; no native encoding or roundtrip gate is used. Controlled futures resolve within four polls. These are floors, not exhaustive input or scheduler enumeration.

Read-only sub-agent review at e020e56507 found no defects in the native bodies, tests, source oracle or four manifests. Reviewer independently ran only the source oracle (exit 0, the same 36/36/24/8 floors) and checked all ten real targets and notes; no Cargo/native tests/mutations were run by the reviewer. No full venue/monorepo/workspace suite, host/wasm Clippy, standalone Rust indexer or CI rerun was performed.

## Account reader post-merge gates — 1c404efaf73bb82cae8865bdab3a932397d913c6

Merged reviewed integration 8aabc56a67ca8ca83a41914a136646154fb8e98c cleanly. Whole Bybit src/tests/scripts retention diff against d5048540f5 exits 0: byte-unchanged case applies. Independent shared build-input enumeration differs only at Cargo.lock, so scoped Cargo check was rerun at 1c404efaf7: exit 0, zero compile errors. The focused account_readers native/source test was rerun there with SAFE_CEX_TYPESCRIPT_PATH set: exit 0, 1 passed / 0 failed / 0 ignored. Extractor check freshly run there exits 0, 7341 symbols / 512 files, zero drift/additions/orphans. Rustfmt --check on the two new Rust files and owned patch whitespace check versus integration both exit 0. Five mutation/restoration exits remain measured at d5048540f5; they were not rerun or relabeled after this merge.

Python 3.11 tomllib census at 1c404efaf7 versus its actual integration merge parent 8aabc56a67 exits 0: exactly ten newly moved rows, 0 pending->ported / 10 pending->CHOSEN divergent; Bybit 121 ported / 99 divergent / 300 pending. Tracked census records both full measured SHAs and original fresh-branch parent 518d08ff3e5cc6800f6f41ecbd08e02a7112ace2. Read-only sub-agent merge audit confirms the same ten real mappings and all retained source/tests/scripts, with no concrete issue. Its first overly strict note comparison rejected the benign accountType note update; the corrected symbol/status/target audit exits 0. Reviewer ran no Cargo/native tests/extractor/mutations for that audit. No new full-suite/workspace/Clippy/indexer/CI claim is made.

## Account reader transport merge gates — 9cbc5f0ae3c58de530b01bf3d88d2ce4517c0085

Pre-push fetch merged integration c5842fe5e72e2d3eb2a734a5c2ad1c82b92bab6e down cleanly. Bybit src/tests/scripts and manifests are byte-unchanged from 1c404efaf7 (retention diffs exit 0), but six shared transport source files changed. At 9cbc5f0ae3 reran scoped Bybit all-targets Cargo check: exit 0, zero compile errors. Reran focused account_readers native/source test with installed TypeScript: exit 0, 1 passed / 0 failed / 0 ignored. Fresh extractor check exits 0: 7341 symbols / 512 files, zero drift/additions/orphans. Owned patch whitespace check exits 0. Python 3.11 tomllib census against pinned integration c5842fe5e7 exits 0, exactly 0 pending->ported / 10 pending->CHOSEN divergent, Bybit 121 / 99 / 300. Tracked census binds these measured full SHAs. Five mutation/restoration exits remain measured at d5048540f5; no additional controls/full-suite/workspace/Clippy/indexer/CI gate is claimed.

## Final account reader retention — c8bbaf4f366fa8eab7d40658fbcd9dddb02078a0

Immediately before push fetched and merged integration 65e086a3c9039a79e4c7e9f354aba31d8f83009e down. Whole Bybit src/tests/scripts/manifests retention diff against scoped-gate SHA 9cbc5f0ae3 exits 0. Shared Cargo/num/types/exchange/store/crypto/transport source, live Bybit/types oracle inputs and extractor source retention diff also exits 0. Byte-unchanged case applies: scoped check, focused native test and extractor exits stay measured at 9cbc5f0ae3 (all 0), five named mutation/restoration exits stay measured at d5048540f5 (101/0 each); none is relabeled as a new run at this evidence revision. Python 3.11 tomllib census against the pinned integration exits 0: exactly ten moves, 0 pending->ported / 10 pending->CHOSEN divergent; totals 121 ported / 99 divergent / 300 pending. The delivery evidence-only commit retains these measured code/test/script/manifest bytes. Original fresh-branch parent remains 518d08ff3e, with no unreviewed slice ancestor at creation. No new workspace/full-suite/Clippy/indexer/CI claim.

## Inverse/spot private lifecycle — 71c0ac12274b98a797f6f479a1d10de7841d71ad

Fresh branch feat/bybit-private-ws-lifecycle was created directly from integration f0cf136efbc17b93fe52369e71342e620dd925ff; no unreviewed feature slice was an ancestor at creation. Initial status found only the user task files, with no unfinished production work or mutation. .aimux-next.md and .aimux-priority.md were read; requested .aimux-brief.md was absent, so the supplied task/evidence rules were used. No CLAUDE.md or agent-memory was read. Python 3.11 tomllib census exits 0: exactly eight independently selected inverse/spot onOpen/ping/subscribe/sendReconnectSequence rows moved, 6 pending->ported / 2 pending->CHOSEN divergent. Baseline 121 ported / 99 divergent / 300 pending; branch 127 / 101 / 292. Tracked evidence/private-ws-lifecycle-census.json lists each actual target. All 26 Bybit manifests were enumerated, stopping at syncUnifiedBalanceUpnlFromPositions.ts.toml; census is not proof of remaining behavior.

Five actual native functions implement the eight method bodies with explicit receiver dependencies. Synchronous onOpen/ping/subscription bodies are ported; per-row notes describe their starter/clock/setter/send boundaries and variant topic differences. Source onOpen does not await the operation its starter schedules. This does not claim full socket construction, dialing, send/notification integration, signer authentication or message handlers; their rows remain pending. Both reconnect rows use CHOSEN notes stating first-poll execution/cancel-on-drop versus source Promise scheduling and already-decoded Rc ExchangeError versus non-Error String coercion/JS stacks/prototypes. Each note identifies its variant and topic composition. Disposal during awaits still permits auth/subscription/snapshot as in source; nested ping alone skips its send after disposal. Thrown auth errors log; false auth directly disregards silence and starts reconnect. No public support types or C3 exemptions were added.

At 71c0ac1227: scoped "${CARGO_BIN}" check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2 exits 0, zero compile errors. Extractor check exits 0: 7341 symbols / 512 files, zero drift/additions/orphans. Writer after mechanically deriving file arrays exits 0, canonicalizing two manifests; writer after the note correction exits 0, zero updates. File-header and target/row assertions exit 0, eight targets / five real functions, six ported / two canonical divergent. Rustfmt --check on the two new Rust files, owned patch whitespace check and exact src/tests/scripts restoration diff all exit 0. Earlier scoped check and extractor check at b259638209 also exited 0; the revised test order is covered by the fresh 71c0ac1227 gates.

At 71c0ac1227: python3.11 sdk-rs/safe-cex-venue-bybit/scripts/private-ws-lifecycle-controls.py exits 0. Its focused native test baseline exits 0 (1 passed / 0 failed / 0 ignored). Five compiling mutations each exit 101 at the named runtime assertion; all five exact restores exit 0 (1 passed / 0 failed / 0 ignored). Tracked evidence/private-ws-lifecycle-controls.json binds every exit/assertion and Cargo command to that full SHA. All Cargo calls were serial with jobs 2, and SAFE_CEX_TYPESCRIPT_PATH pointed to ${DEPENDENCY_ROOT}/node_modules/typescript. Selected control floor is five, stopped at reconnect delay (iteration 5). Zero survivors and zero invalid controls in this run.

| Family | Deliberate mutation | Named runtime failure | Mutation / restore |
| --- | --- | --- | --- |
| onOpen | Drop starter without calling it | LIFECYCLE native open start count | 101 / 0 |
| Ping | Add one to performance timestamp | LIFECYCLE native ping timestamp | 101 / 0 |
| Inverse subscription | Swap order/position topics | LIFECYCLE native subscription topic | 101 / 0 |
| Spot subscription | Send position instead of order | LIFECYCLE native subscription topic | 101 / 0 |
| Reconnect | Sleep 26 instead of 25 | LIFECYCLE native sleep delay | 101 / 0 |

Live-source oracle floors: four open cases (stopped at index 3), eighty ping cases (79), four subscription cases (3), fifty-two reconnect cases (51). Actual maintained inverse/spot class members are uniquely selected by AST and evaluated; actual ping/subscription bodies compose inside reconnect. Native timestamp IEEE bits (including negative zero, NaN and infinity), decoded op/ordered topic fields, state/counts, original/logged Rc error identity and name/message/code, sleep delay and partial failure state are asserted directly. Every native assertion across all fixtures runs before the first raw wire comparison; no native re-encoding/roundtrip comparison is used. Controlled futures settle within five polls; this is a bounded scheduler/input floor, not exhaustive coverage. Pending reconnect/snapshot starter promises do not block source completion.

Read-only sub-agent review found a note error (false auth incorrectly described as logging); both notes were corrected. Final review at 71c0ac1227 found no remaining issue, confirmed six ported / two CHOSEN real targets and all-native-before-wire ordering. Reviewer ran a source-only oracle (exit 0, the same floors) and read-only mapping/retention checks, but no Cargo/native tests/extractor/mutations. During the test-order edit rustfmt first exited 1 for an unclosed test helper; the helper was restored from committed code and formatting passed before 71c0ac1227. This development formatting failure was not a mutation control; no broken revision was committed. The initial development native test exited 0 before the implementation commit.

Not run: full venue suite, full monorepo/workspace build or tests, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live exchange/network integration or full constructor/auth composition. No PR was opened or merged by this agent.

## Private lifecycle merge retention — e9dda937298eaf761b21d478aa73b2bd9ee1ee5c

Merged integration 3205aab9c98d146c7c43e4f48b64f7d4b636cbf8 down cleanly. Whole Bybit src/tests/scripts/manifests diff --exit-code against measured gate 71c0ac1227 exits 0: byte-unchanged case applies. Independent shared build/source-input enumeration differs only at Cargo.lock; its sole diff adds safe-cex-candle-cache to safe-cex-venue-apexomni. Python 3.11 resolves every transitive Bybit lock dependency and asserts the before/after package-record dictionaries equal: exit 0, 481 records visited, stopped after the final queued dependency. That lock-only unrelated change does not change the Bybit dependency closure. Scoped Cargo check, native baseline and five mutation/restoration exits stay measured at 71c0ac1227; none was rerun or relabeled at this merge. Fresh extractor check at e9dda937298eaf761b21d478aa73b2bd9ee1ee5c exits 0, 7341 symbols / 512 files, zero drift/additions/orphans. Python 3.11 tomllib census against pinned integration exits 0: exactly 6 pending->ported / 2 pending->CHOSEN divergent, eight rows; totals 127 ported / 101 divergent / 292 pending. Original branch parent remains f0cf136efb. No new full-suite/workspace/Clippy/indexer/CI/network claim.

## Final private lifecycle pre-push retention — 6f1540ef2968398c70ebcdb2f0f49db0de4aee55

Immediately before delivery fetched and merged integration c1b0ce61f2247f5604b26ed5a00b142be75d90ef down. Whole Bybit src/tests/scripts/manifests diff --exit-code against 71c0ac1227 exits 0; shared source/Cargo/lock/oracle/extractor inputs diff versus previously audited merge e9dda93729 also exits 0. Byte-unchanged case applies. Scoped Cargo check, native test baseline and all five named mutation/restoration exits remain measured at 71c0ac1227 (check/test 0; five 101/0 pairs), not relabeled as new runs at this delivery revision. Fresh extractor check at 6f1540ef2968398c70ebcdb2f0f49db0de4aee55 exits 0, 7341 symbols / 512 files, zero drift/additions/orphans. Final Python 3.11 tomllib census against pinned integration exits 0: exactly 6 pending->ported / 2 pending->CHOSEN divergent, eight rows; totals 127 ported / 101 divergent / 292 pending. Read-only sub-agent merge review confirms all retained mappings and corrected notes, no concrete issue; no reviewer Cargo/extractor/mutations. Original branch parent remains f0cf136efb, with no unreviewed feature ancestor at creation. The evidence-only delivery commit retains these measured input bytes. Full workspace/venue suites, host/wasm Clippy, standalone Rust indexer, CI and live-network/constructor/auth integration were not run.

## API validation and VIP metadata — 24cea8b7e86645a4b6541d0cc68bfc8354081dbe

Fresh branch feat/bybit-api-validation was created directly from integration 41505c870788695cb1e3201843360997f3d22794, with no unreviewed feature ancestor. Initial status found only user task files and no inherited unfinished code/mutation. Read .aimux-next.md and .aimux-priority.md; .aimux-brief.md was absent. No CLAUDE.md or agent-memory was read.

Python 3.11 tomllib census at 24cea8b7e86645a4b6541d0cc68bfc8354081dbe exits 0: exactly ten independently selected pending rows moved, 2 pending->ported / 8 pending->CHOSEN divergent. Bybit changes from 127 ported / 101 divergent / 292 pending to 129 / 109 / 282. Tracked evidence/api-validation-census.json pins the measured SHA, original parent and real targets. All 26 manifests / 520 rows were enumerated, stopping at syncUnifiedBalanceUpnlFromPositions.ts.toml. Census is separate from runtime proof.

Seven actual native functions implement primary/inverse/spot/Hyro validation, primary/inverse API info fetch/cache and both VIP batch bodies. VIP bodies are ported with declared string VIP getter rereads, actual typed rate-table values, strict UNIFIED account selection and upgrade-only limit setter ordering. Eight async rows have individual CHOSEN notes for explicit host callbacks, anonymous object/tuple adaptation, immutable plain JSON without executable/getter/prototype/mutable-alias behavior, Rc errors without JavaScript stacks/prototypes, and Future first-poll/cancel-on-drop versus Promise scheduling. Primitive/null top-level fetch responses are excluded: source v(true, retCode) throws TypeError whereas native lookup treats it as missing. Captured retMsg is not reread after emission; response mutation during callbacks is excluded. Full constructors, typed cache/store/transport and balance formatter composition remain pending; no support-type/C3 exemptions were added.

At 0d2836965be0f79bc4cb83d844c11f0a4ed40dd1, scoped "${CARGO_BIN}" check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2 exits 0, zero compile errors. Extractor check there also exits 0. At 24cea8b7e86645a4b6541d0cc68bfc8354081dbe, fresh extractor check exits 0: 7341 symbols / 512 files, zero drift/additions/orphans. Source/tests/scripts diff against 0d2836965b exits 0; only two boundary notes changed. Development writer exits 0 after initial manifest edits (four canonicalized files), and again after the note update (zero updates); headers were mechanically derived from real targets. Formatting and owned patch whitespace checks exit 0.

At 24cea8b7e86645a4b6541d0cc68bfc8354081dbe, python3.11 sdk-rs/safe-cex-venue-bybit/scripts/api-validation-controls.py exits 0. Baseline focused api_validation native/source test exits 0, 1 passed / 0 failed / 0 ignored. Seven compiling mutations each exit 101 at a named runtime assertion; all seven exact restores exit 0, 1 passed / 0 failed / 0 ignored. Tracked evidence/api-validation-controls.json pins every exit and command to that SHA. Cargo calls were serial with jobs 2 and SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript. Control enumeration floor is seven, stopped at VIP batch limit, iteration 7. Zero survivors/invalid controls.

| New native family | Named failing assertion | Mutation / restore |
| --- | --- | --- |
| primary validation | API native scalar validation error | 101 / 0 |
| derived validation | API native scalar validation error | 101 / 0 |
| Hyro validation | API native Hyro error | 101 / 0 |
| API key fetch | API native rejection | 101 / 0 |
| primary cache namespace | API native user-data key | 101 / 0 |
| inverse cache namespace | API native user-data key | 101 / 0 |
| VIP batch limit | API native number VIP batch limit | 101 / 0 |

Mutations moved each validation balance invocation across its catch boundary, replaced Hyro error text, inverted fetch success detection, replaced each variant store key, and set batch limit 21 instead of 20. Actual maintained TypeScript class members are uniquely selected by AST and evaluated with the maintained endpoints, v lookup, enums/table; constructor code is not evaluated. Independent live-source floors: validations 60 (stopped at index 59), Hyro 6 (5), fetches 28 (27), caches 32 (31), VIP cases 70 (69), 196 selected fixtures. Every assertion compares decoded native fields/optional absence, direct f64 bits, original Rc identity, callback/state counts and order; no roundtrip, native re-encoding or raw wire comparison is used. Controlled futures settle within five polls; neither input nor scheduling coverage is claimed complete.

Read-only sub-agent review found a test defect: absent balance fields were indexed as null. It was corrected with .get() and an explicit undefined sentinel preserving actual null. Review also found the primitive fetch-response difference above; both row notes now explicitly exclude that domain. Final note/mapping review exits 0: ten real mappings, 2 ported / 8 CHOSEN, seven functions, matching file-header coverage and typed VIP mapping. Reviewer source-only oracle exits 0 and its in-memory primitive supplement exits 0; no reviewer Cargo/extractor/mutations. Initial uncommitted compilation exited 101 for inferred error type and typed-table indexing; these were fixed before commits. The pre-fix development native test exited 101 at API native present validation balance; corrected development test exited 0 before the implementation commit. These development failures are not mutation controls.

Not run: full Bybit/venue suite, workspace or monorepo build/tests, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live exchange/network calls or full constructor/auth/typed cache/store composition. No PR was opened or merged by this agent. Queue skill completion check reports nothing outstanding; this in-flight task was not added to the queue.

## API validation merge gates — b68da947814773d837590c7aacb52c9d087956ff

Merged integration b5d642aea03ced39a3cc27bb95767b8fd242c3b7 down cleanly. Whole Bybit src/tests/scripts/manifests retention diff versus 24cea8b7e8 exits 0. Shared exchange cross-trade/order-operation source changed, so fresh scoped Bybit all-targets Cargo check and focused api_validation native/source test were run at b68da947814773d837590c7aacb52c9d087956ff: both exit 0, test 1 passed / 0 failed / 0 ignored. Fresh extractor check at that SHA exits 0, 7341 symbols / 512 files, zero drift/additions/orphans. Python 3.11 tomllib census against pinned integration exits 0: ten moves, 2 pending->ported / 8 pending->CHOSEN divergent; Bybit 129 / 109 / 282. Updated tracked census pins both full SHAs and the original parent 41505c8707. Seven mutation/restoration exits remain measured at 24cea8b7e8, 101/0 each; they were not rerun or relabeled at the merge. Owned native bodies/tests/oracle/control scripts are byte-unchanged. No new workspace/full-suite/Clippy/indexer/CI/live-network/composition claim.

## Final API validation delivery retention — c6efd685303ef431b2e76993f3f6f7469c3cb78e

Immediately before push fetched and merged integration 3042a36afe21890c7cb5eaa5ba78333f35e5bd40 down. Whole Bybit src/tests/scripts/manifests retention diff against 24cea8b7e8 exits 0. Shared Cargo/lock/num/exchange/types/store/crypto/transport source, live Bybit/utils/types oracle inputs and extractor source retention diff against b68da94781 also exits 0. Byte-unchanged case applies: scoped check and focused native/source test exits stay measured at b68da94781 (0/0), seven named mutation/restoration pairs at 24cea8b7e8 (101/0 each); no fresh Cargo or controls are claimed for this final merge. Fresh extractor check at c6efd685303ef431b2e76993f3f6f7469c3cb78e exits 0, 7341 symbols / 512 source files, zero drift/additions/orphans. Final Python 3.11 tomllib census against pinned integration exits 0: ten moves, 2 pending->ported / 8 pending->CHOSEN divergent; totals 129 ported / 109 divergent / 282 pending. Tracked census records these full measured/base SHAs. This delivery evidence-only commit retains the measured inputs; original parent remains 41505c8707 with no unreviewed feature ancestor at creation. Full venue/workspace suites, host/wasm Clippy, standalone Rust indexer, CI and live-network/full composition were not run. No PR was opened or merged by this agent.

## Inverse/spot public ticker-topic bodies — f2b752be331048708d12ba2854a2dde08badfbf8

Fresh branch feat/bybit-public-ticker-topics was created directly from integration ef054ba0b8e9038241576f44c09f652348f2f4bb, without an unreviewed feature ancestor. Initial status found only untracked user task files: no inherited code or broken mutation. Read .aimux-next.md and .aimux-priority.md; .aimux-brief.md was absent. No CLAUDE.md or agent-memory was read.

Python 3.11 tomllib census at f2b752be331048708d12ba2854a2dde08badfbf8 exits 0: exactly ten independently selected pending rows moved, 8 pending->ported / 2 pending->CHOSEN divergent. Bybit changes from 129 ported / 109 divergent / 282 pending to 137 / 111 / 272. Tracked evidence/public-ticker-topics-census.json pins the measured/base/original-parent SHAs and real targets. Both sides of the census are read from committed git objects, so uncommitted manifests cannot be mislabeled as HEAD. Enumerated all 26 Bybit manifests / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml; census is separate from runtime proof.

Five actual native functions map the ten inverse/spot bodies. Eight synchronous rows preserve the declared string/string-array topic domain and shared UTF-8 boundary: current arrays retain mutable Rc/RefCell identity, desired lists preserve duplicates and empty symbols, ordered union snapshots current before desired and keeps existing subscriptions, and active-symbol changes send only added topics in ordered chunks of ten after synchronization/connected lookup. Generic explicit receiver callbacks retain original failure identity and partial setter state. Full classes, constructors, connection/dialing, event registration and other methods remain pending; no support types or C3 exemptions were added. Both async onMarketsChanged notes are CHOSEN: owned symbol-list projection excludes executable/sparse market getters and market-object alias mutation, and Future first-poll/cancel-on-drop differs from Promise scheduling. Native body preserves guard order, duplicate-preserving append, state assignment before sends, and five-ms sleep after every chunk, including the last; disposal during a sleep does not add a new guard.

At f2b752be331048708d12ba2854a2dde08badfbf8, scoped "${CARGO_BIN}" check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2 exits 0, zero compile errors. Extractor check exits 0: 7341 symbols / 512 source files, zero drift/additions/orphans. The writer after mechanically derived header edits exits 0 before the implementation commit (two canonicalized manifests); it is a development writer run, not relabeled as a committed gate. Initial check at 4b34983d528a1866d8b4a3935e5a04ff7bb48d68 also exits 0; only the census reader changed afterward and src/tests retention versus that SHA exits 0. Rustfmt --check and patch whitespace checks exit 0. Initial uncommitted focused native/source test exits 0; no pre-gate development compilation/test failure occurred in this slice.

At f2b752be331048708d12ba2854a2dde08badfbf8, python3.11 sdk-rs/safe-cex-venue-bybit/scripts/public-ticker-topics-controls.py exits 0. Its focused public_ticker_topics baseline native/source test exits 0, 1 passed / 0 failed / 0 ignored. Five compiling controls each exit 101 at a named runtime assertion; every exact restore exits 0, 1 passed / 0 failed / 0 ignored. Tracked evidence/public-ticker-topics-controls.json pins commands, names, exits and measured SHA. Cargo calls were serial with jobs 2 and SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript. Control floor is five, stopped at market chunk sleep, iteration 5. Zero survivors/invalid controls; exact original production bytes restored.

| New native family | Mutation | Named assertion | Mutation / restore |
| --- | --- | --- | --- |
| current array identity | Clone the current array instead of returning its identity | TOPICS native current array identity | 101 / 0 |
| desired topic prefix | Use ticker. instead of tickers. | TOPICS native desired topic | 101 / 0 |
| ordered deduplication | Keep duplicate topics | TOPICS native topic count | 101 / 0 |
| active chunk order | Reverse active subscription chunk order | TOPICS native topic count | 101 / 0 |
| market chunk sleep | Sleep six milliseconds instead of five | TOPICS native sleep delay | 101 / 0 |

Live-source independent floors: current topics 20 (stopped at index 19), desired topics 16 (15), sync 20 (19), active symbols 50 (49), market additions 36 (35), 142 selected fixtures total. Maintained inverse/spot class members are uniquely selected by AST and evaluated with actual lodash/chunk; constructor code is not evaluated. Native arrays/scalars, mutated current/original/received state, Rc array/error identity, original error name/message/code, decoded frame op/ordered args, callback order/counts and integer sleep delays are asserted directly before ALL raw wire comparisons. No native re-encoding or roundtrip comparison is used. Native futures settle within five polls, a bounded input/scheduling floor rather than exhaustive behavior.

Read-only sub-agent review found no production/test defect in the declared string/string-array domain. Source-only oracle exits 0 with the same five fixture floors. Final mapping/header/notes script exits 0, ten real mappings / five functions / eight ported / two canonical CHOSEN. Reviewer found census SHA binding read working-tree bytes; it was corrected to git show of committed HEAD in f2b752be33, and the corrected census exits 0. Reviewer ran no Cargo/native tests/extractor/mutations. Queue skill completion check reports nothing outstanding; this current task was not added to the queue.

Not run: full venue/Bybit suite, workspace/monorepo build or tests, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live exchange/network calls or full socket constructor/dialing/event-loop composition. No PR was opened or merged by this agent.

## Public ticker-topic merge retention — dc4d13ef0396e16697e6e03dab7021be7d8cdbaa

Merged integration 3a957718fe445a833bb10a464e16e7cc6c585276 down. Whole Bybit src/tests/scripts/manifests retention diff against f2b752be33 exits 0. Shared Cargo/lock/num/exchange/types/store/crypto/transport inputs, live Bybit/types source and extractor code retention also exits 0. Byte-unchanged case applies: scoped Cargo check and native baseline remain measured at f2b752be33 (0/0), five named mutation/restoration pairs remain 101/0 there; no fresh Cargo/controls are claimed at this merge. Fresh extractor check at dc4d13ef0396e16697e6e03dab7021be7d8cdbaa exits 0: 7341 symbols / 512 files, zero drift/additions/orphans. Fresh committed-object Python 3.11 census against pinned integration exits 0: exactly ten moves, 8 pending->ported / 2 pending->CHOSEN divergent; Bybit 137 / 111 / 272. Updated tracked census records both full measured/base SHAs and the original parent ef054ba0b8. Original branch creation had no unreviewed feature ancestor. No new full-suite/workspace/Clippy/indexer/CI/network/composition claim.

## Final public ticker-topic delivery retention — 1805a2286a3c37e1513d6886350c3bf74651a141

Final pre-push fetch merged integration 6a15df74a80721009ec4b07ea1c4236c1d3242e7 down. Whole Bybit src/tests/scripts/manifests and shared Cargo/lock/num/exchange/types/store/crypto/transport source, live Bybit/types oracle inputs and extractor source diff --exit-code versus f2b752be33 both exit 0. Byte-unchanged case applies. Scoped check, native baseline and all five mutation/restoration pairs stay measured at f2b752be331048708d12ba2854a2dde08badfbf8: check/native 0/0, controls 101/0 each; none is relabeled as a fresh Cargo run here. Fresh extractor check at 1805a2286a3c37e1513d6886350c3bf74651a141 exits 0, 7341 symbols / 512 source files, zero drift/additions/orphans. Final committed-object tomllib census against pinned integration exits 0: exactly ten pending moves, 8 pending->ported / 2 pending->CHOSEN divergent, totals 137 ported / 111 divergent / 272 pending. Tracked census pins both full SHAs and original fresh-branch parent ef054ba0b8. This evidence-only delivery commit retains those measured inputs. No unreviewed feature ancestor at original branch creation. Full venue/workspace suites, host/wasm Clippy, standalone Rust indexer, CI reruns, live-network/full socket composition were not run; no PR was opened or merged by this agent.

## Inverse/spot public socket lifecycle — e07609b636ba8d45f8c7f50f4e176077758cf83b

Fresh branch feat/bybit-public-ws-lifecycle was created directly from integration 59fab229a563a163c7ffe831c844da1c1149b5a2, without an unreviewed feature ancestor. Initial status found only user task files and no unfinished code/mutation. Read .aimux-next.md; .aimux-priority.md instructions were retained from the prior slice and .aimux-brief.md remains absent. No CLAUDE.md or agent-memory was read.

Python 3.11 committed-object tomllib census at e07609b636ba8d45f8c7f50f4e176077758cf83b exits 0: exactly ten independently selected pending rows moved, 6 pending->ported / 4 pending->CHOSEN divergent. Bybit changes from 137 ported / 111 divergent / 272 pending to 143 / 115 / 262. Tracked evidence/public-ws-lifecycle-census.json pins measured/base/original-parent SHAs and real targets. Enumerated all 26 Bybit manifests / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml. Census is separate from runtime proof.

Five actual native functions map the ten inverse/spot connectAndSubscribe/onOpen/dispose/ping/subscribe bodies. Six synchronous open/dispose/ping rows are ported with explicit ordinary receiver callbacks, original handler/error identity, unawaited subscription starter, handler rereads across off/on, unregister-before-base-dispose, and exact performance f64 recording before public send. The starter reports synchronous invocation failure; its independently scheduled completion/rejection remains unawaited. Both connection rows have CHOSEN notes for typed owned route tuple/category projection, omitted unused reconnect flag and excluded executable/prototype member lookup or route-field mutation during resolveUrl. Generic options/socket/handler values retain native identity; proxy notification errors are deliberately swallowed and each listener socket receiver is captured before its handler read. Both subscription rows have CHOSEN notes for owned plain-map/dense topic projection, excluded executable/sparse/non-JSON aliases, and Future first-poll/cancel-on-drop versus Promise scheduling. Native one-level flattening uses ECMAScript numeric-key order and ten-topic chunks with ten-ms sleep after every chunk including the last. Constructors/full classes/dialing/host event-loop composition remain pending; no support types or C3 exemptions were added.

At e07609b636ba8d45f8c7f50f4e176077758cf83b, scoped "${CARGO_BIN}" check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2 exits 0, zero compile errors. Extractor check exits 0: 7341 symbols / 512 source files, zero drift/additions/orphans. Development writer after manifest edits exits 0 before the commit (two canonicalized files), with header arrays mechanically derived from actual row targets; it is not relabeled as a committed gate. Rustfmt --check and patch whitespace checks exit 0. Initial pre-fix development native test exits 0 for 208 cases; corrected pre-commit native test exits 0 for 210. No development compilation/test failure occurred in this slice.

At e07609b636ba8d45f8c7f50f4e176077758cf83b, python3.11 sdk-rs/safe-cex-venue-bybit/scripts/public-ws-lifecycle-controls.py exits 0. Focused public_ws_lifecycle native/source baseline exits 0, 1 passed / 0 failed / 0 ignored. Five compiling mutations each exit 101 at a named runtime assertion; all exact restores exit 0, 1 passed / 0 failed / 0 ignored. Tracked evidence/public-ws-lifecycle-controls.json pins names, exits, command and measured SHA. Cargo invocations were serial with jobs 2 and SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript. Selected control floor is five, stopped at subscribe delay, iteration 5; zero survivors/invalid controls. Original production bytes exactly restored.

| Native family | Mutation | Named assertion | Mutation / restore |
| --- | --- | --- | --- |
| connect listener receiver order | Read the handler before capturing listener receiver | PUBLIC native registrations | 101 / 0 |
| open topic sync | Skip synchronous topic synchronization | PUBLIC native callback count | 101 / 0 |
| dispose listener key | Change the active-symbol event name | PUBLIC native listener key | 101 / 0 |
| ping timestamp | Add one to the clock value | PUBLIC native number ping timestamp | 101 / 0 |
| subscribe delay | Sleep eleven milliseconds instead of ten | PUBLIC native subscription delay | 101 / 0 |

Independent maintained-source floors: connect 96 (stopped at index 95), open 38 (37), dispose 10 (9), ping 36 (35), subscribe 30 (29), 210 selected fixtures. Actual inverse/spot class members and their variant BASE_WS_URL declarations are selected uniquely by AST and evaluated with maintained lodash/chunk/flatten. Constructor/base/transport/site-option dependencies are explicit fixture receivers, not a live dialing claim. Native callback order/fields, registrations and committed state, original error name/message/code and Rc identity, socket/option/handler identity, undefined protocol slot, IEEE timestamp bits including NaN/both infinities/negative zero, integer delays and decoded frame op/ordered args are asserted before ALL raw wire comparisons. No native re-encoding or roundtrip comparison is used. Controlled futures settle within five polls, a bounded scheduling/input floor.

Read-only sub-agent review found receiver evaluation order: source captures this.ws before evaluating the handler, whereas the initial adapter read it inside listen afterward. A source-only getter-replacement supplement exits 0 and showed open must register on SOCKET-A while later registrations use SOCKET-B. Added an explicit receiver callback before the handler and promoted the witness from 208 to 210 cases. The named receiver-order mutation above then failed PUBLIC native registrations (101), and exact restore passed (0), proving this regression witness discriminates. Final review finds no remaining concrete defect, verifies ten real mappings / five functions / six ported / four canonical CHOSEN and matching headers. Source-only oracle exits 0 for 210 cases. Reviewer ran no Cargo/native tests/extractor/mutations. Queue completion check reports nothing outstanding; this in-flight task was not added to the queue.

Not run: full Bybit/venue suite, workspace/monorepo builds or tests, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live exchange/network calls or full socket constructor/transport/host scheduling composition. No PR was opened or merged by this agent.

## Public socket lifecycle merge retention — c10a21c42a7d16bbc41f398cb480a1c5fd64687c

Merged integration 5911167261853a8cbcc99d79cf45b785bb016648 down cleanly. Whole Bybit src/tests/scripts/manifests retention diff against e07609b636 exits 0. Shared source/build/oracle/extractor inputs differ only at Cargo.lock: the sole patch adds indexmap 2.14.2 and safe-cex-crypto to safe-cex-venue-okx. Independent Python 3.11 lock traversal resolves all transitive Bybit dependencies and compares complete package records before/after: exit 0, 481 records equal, stopped at windows_x86_64_msvc 0.52.6. This is a lock-closure audit, not Windows compilation. Scoped check, native baseline and five named mutation/restoration pairs remain measured at e07609b636 (check/native 0/0; controls 101/0 each), not rerun at this merge. Fresh extractor check at c10a21c42a7d16bbc41f398cb480a1c5fd64687c exits 0, 7341 symbols / 512 files, zero drift/additions/orphans. Fresh committed-object tomllib census against pinned integration exits 0: ten moves, 6 pending->ported / 4 pending->CHOSEN divergent; totals 143 / 115 / 262. Tracked census pins these full measured/base SHAs and original parent 59fab229a5. No new workspace/full-suite/Clippy/indexer/CI/network/composition claim.

## Final public socket lifecycle delivery retention — 96685079aeecfd0ca68f6ea30d9439a699b04acf

Final pre-push fetch merged integration 91bdde133d2f407dffa8f2140f94e87eac7c1d5f down. Whole Bybit src/tests/scripts/manifests diff --exit-code against e07609b636 exits 0. Shared Cargo/lock/num/exchange/types/store/crypto/transport source, live Bybit/types oracle and extractor inputs diff versus the previously audited c10a21c42a merge also exits 0. Byte-unchanged case applies; the already-audited OKX-only lock change still leaves all 481 Bybit dependency records unchanged. Scoped check, native baseline and five mutation/restoration pairs remain measured at e07609b636ba8d45f8c7f50f4e176077758cf83b: check/native 0/0, controls 101/0 each. They were not rerun or relabeled at this delivery merge. Fresh extractor check at 96685079aeecfd0ca68f6ea30d9439a699b04acf exits 0, 7341 symbols / 512 files, zero drift/additions/orphans. Final committed-object tomllib census against pinned integration exits 0: exactly ten pending moves, 6 pending->ported / 4 pending->CHOSEN divergent, totals 143 ported / 115 divergent / 262 pending. Tracked census pins full measured/base SHAs and original fresh-branch parent 59fab229a5, with no unreviewed feature ancestor at creation. This evidence-only delivery commit preserves those measured inputs. Full venue/workspace suites, host/wasm Clippy, standalone Rust indexer, CI reruns, live-network/full socket composition were not run; no PR was opened or merged by this agent.

## Position controls — primary and inverse, ten pending method bodies

Fresh branch `feat/bybit-position-controls` starts directly at reviewed integration `d5c0e7d58a7f03c8163680335e7b632ebb43ec86`; no unreviewed feature branch was an ancestor at creation. Initial status found only the two untracked task instructions, with no inherited code/mutation work. `.aimux-next.md` and `.aimux-priority.md` were read; `.aimux-brief.md` is absent. CLAUDE.md and agent-memory were not reread.

Ported actual bodies are setLeverage, changePositionMode, changePositionModeHelper, changeMarginType and fetchPositionMode in both maintained classes. Seven native public functions provide these ten bodies with explicit host dependencies. All ten rows move pending->CHOSEN divergent: first-poll/cancel-on-drop Future scheduling differs from eager promises; per-row notes additionally name immutable leverage-limit snapshots, optional undefined request-map key omission, retained JSON/property boundaries and omitted mapper array argument where applicable. Full class/constructor wiring remains pending. Source's discarded position-mode symbol, three helper arguments, primary not-modified exemption versus inverse strict OK, store refresh before mode response checks and margin assignment before position refresh are preserved.

| Measurement | Exact measured SHA | Exit / output |
| --- | --- | --- |
| Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2` | `528137bc81faaf682fa25f5a376784e4734f1c39` | 0, Finished dev profile |
| Extractor check via explicit Node/Yarn invocation | `528137bc81faaf682fa25f5a376784e4734f1c39` | 0; 7341 symbols / 512 files; zero drift/additions/orphans |
| Committed-object tomllib census | `a7f44a5025b91ba1b07fe5253d024828de9fd655` | 0; 0 pending->ported, 10 pending->CHOSEN divergent; 143 ported / 125 divergent / 252 pending from 143 / 115 / 262 |

The initial census at `528137bc81` exited 0 with 2 ported / 8 divergent moves. Review caught that the wrappers share the scheduling difference, so this classification was corrected at `a7f44a5025`; the final census above supersedes it. Source/tests retention diff from `528137bc81` through `a7f44a5025` exits 0, preserving the measured scoped check without relabeling its SHA. The extractor writer was run after both manifest edits and reported zero additions/orphans; those dirty-tree writer runs are not substitute check gates.

The maintained-source oracle extracts each exact class member via AST, evaluates it with explicit host fixtures, and uses maintained v/response metadata/constants and leverage-not-modified declarations. Constructors are excluded. Independent floors: leverage 52, wrapper 10, helper 30, margin 40, fetch mode 50; total 182, stopped at global iteration 181 (inverse fetch/emit failure), with per-family input iterations 25 / 4 / 14 / 19 / 24 in each variant. Native futures stop by poll five. Tests inspect decoded native event/request/mapped-value fields, IEEE number values, margin/store state, fetched batch allocation identity and original callback-error identity. An explicit success/rejection discriminant precedes result field assertions. No native encode/roundtrip or wire assertion is used.

Development observations are not release gates: initial native compilation exited 101 for opaque Future lifetime capture; precise lifetime capture fixed it. Then a named native wrapper array-count assertion exited 101 because the fixture passed a fourth absent helper argument; corrected to the source's exact three arguments and the focused test exited 0. Read-only sub-agent review caught the result-discriminant gap (possible successful null matching a rejection); it was corrected before the committed runtime gates. Final committed-object review of `a7f44a5025` found no remaining issue within declared boundaries and verified all ten real mappings, seven functions and independent census expectations. Reviewer ran no Cargo or controls.

An invalid first leverage control at `528137bc81` exited 101 at compile time due to deny-warnings unused imports/variables; its exact restore exited 0. It does not establish discrimination. The retained [invalid-control artifact](evidence/position-controls-invalid-control.json) records this finding and the replacement clamp mutation. Final runtime controls are recorded separately below.

Focused native command: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript "${CARGO_BIN}" test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test position_controls --jobs 2 -- --include-ignored`. Baseline and all five exact restores at `a7f44a5025b91ba1b07fe5253d024828de9fd655` exited 0 with one test passed / zero failed / zero ignored, each executing all 182 cases. The sequential runner exited 0; all five compiling mutations failed at their named runtime assertion with 101. No runtime survivor. Final working-tree source/tests retention diff against `a7f44a5025` exits 0.

| Family | Named native assertion | Mutation exit | Exact restore exit |
| --- | --- | --- | --- |
| leverage clamp | `POSITION native leverage event` | 101 | 0 |
| mode bulk route | `POSITION native mode event` | 101 | 0 |
| mode helper hedged index | `POSITION native helper event` | 101 | 0 |
| margin setter | `POSITION native margin event` | 101 | 0 |
| first fetched mode | `POSITION native fetch result` | 101 | 0 |

Machine-readable [runtime controls](evidence/position-controls-controls.json) and [committed census](evidence/position-controls-census.json) pin measured SHAs. The control enumeration floor is five, stopped on first fetched mode. Queue completion check exited 0 with nothing outstanding; this task was not inserted into the queue.

Not run: full Bybit/venue test suite, workspace or monorepo builds/tests, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live exchange/network requests or full class/constructor/transport/promise scheduling composition. No shared type or trait was changed; no PR was opened or merged by this agent.

Fresh extractor check at `a7f44a5025b91ba1b07fe5253d024828de9fd655` exited 0 after all final row classifications: 7341 symbols / 512 files, zero drift/additions/orphans. Output text was inspected; the Yarn process exit alone was not the criterion.

## Position controls merge retention — 6dd1227ca2b54d0432f8b746da6a30dccbcc598d

Fetched and merged reviewed integration `11c26bc6a7bb6240e836df4dcf5cb6ed73ef107f` down cleanly. Separate `git diff --exit-code` measurements against `a7f44a5025b91ba1b07fe5253d024828de9fd655` exited 0 for whole Bybit src/tests/scripts/Cargo manifest/parity manifests and 0 for inspected shared Cargo/lock/config, num/exchange/types/store/crypto/transport source/manifests/build scripts, maintained Bybit/types/get-key oracle inputs and extractor inputs. An additional manifest-driven traversal of local workspace/path dependencies also exited 0 for retained source/Cargo/build.rs: floor eight packages (Bybit, candle-cache, crypto, exchange, num, store, transport, types), stopped at safe-cex-types. This is retention evidence, not workspace compilation. Byte-unchanged case applies: scoped check remains measured at `528137bc81`, native baseline and five runtime controls/restores at `a7f44a5025`; those gates were not rerun or relabeled at this merge.

Fresh extractor check at `6dd1227ca2b54d0432f8b746da6a30dccbcc598d` exited 0: 7341 symbols / 512 files; zero drift/additions/orphans. Fresh committed-object tomllib census against pinned integration `11c26bc6a7bb6240e836df4dcf5cb6ed73ef107f` exited 0: exactly ten moves, 0 pending->ported / 10 pending->CHOSEN divergent; Bybit totals 143 ported / 125 divergent / 252 pending from 143 / 115 / 262. Manifest floor is 26 files / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml. The tracked census pins both full measured/base SHAs and the original reviewed integration parent. No workspace/full-suite/Clippy/indexer/CI/network/composition claim; this following evidence-only commit preserves all measured gate inputs.

## Exchange stream delegation and disposal — eleven matching synchronous bodies

Fresh branch `feat/bybit-stream-delegation` starts directly at reviewed integration `648fe274091ac68cfae9bfd65fbbaeabd6e0ae0d`, with no unreviewed feature branch ancestor at creation. Initial status/log found only the two untracked task instruction files and no inherited code or mutation. `.aimux-next.md` was read; retained priority instructions apply. CLAUDE.md and agent-memory were not reread.

Eleven pending rows now have real native targets: primary/inverse/spot listenOHLCV, listenOrderBook and dispose, plus primary/inverse listenOHLCVs. All eleven are **ported**, across nine public functions in stream_delegation.rs. These synchronous bodies preserve public versus inverse receiver routing, original option/callback/receiver/return/error handles, the bulk wrapper's deliberately discarded return, and base/public/private disposal ordering. Primary disposal cancels the debounced refresh and clears each live timer key before base disposal. No blanket async/JSON ABI divergence is inferred: these bodies contain no async scheduling or JSON projection. Full exchange/socket constructors and called dependency implementations remain separate rows.

| Gate | Exact measured SHA | Exit / observed output |
| --- | --- | --- |
| Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2` | `d757187187674f71c1b38b0802c5567545a49be2` | 0, Finished dev profile |
| Extractor check via explicit Node/Yarn command | `d757187187674f71c1b38b0802c5567545a49be2` | 0; 7341 symbols / 512 files; zero drift/additions/orphans |
| Committed-object tomllib census | `926d8916f2b5932ff46c93c9c2aefca08e3d7a37` | 0; 11 pending->ported / 0 pending->divergent; 154 ported / 125 divergent / 241 pending from 143 / 125 / 252 |

The first committed census at `d757187187` also exited 0 with the same eleven moves and totals. Read-only review corrected note accuracy only: order-book cases preserve the immutable symbol rather than mutate arguments; inverse/spot dispose calls no timer helper. Writer was rerun after these note edits, reporting zero additions/orphans. Separate source/tests/scripts retention diff `d757187187` to `926d8916f2` exited 0, preserving the measured scoped check without relabeling its SHA. No development or measured gate failure was observed before the controls.

The exact maintained TypeScript class members are selected uniquely by AST and executed with explicit host dependencies; the base dispose and socket/timer dependencies are test hosts, with class construction excluded. Native tests assert decoded return/event/state leaves, options/callback/receiver/return/error identity, real callback invocation and callback rejection, mutable nested option forwarding, method-getter receiver replacement, base/public disposal socket replacements, and live Map-key addition/deletion/reinsertion. Success versus rejection is asserted before result fields. No native serialization, re-encoding, roundtrip or wire assertion is used.

Independent floors are thirteen cases for each of eight delegation symbols, nineteen primary disposal cases and eight each inverse/spot disposal: total 139, stopped at iteration 138 (spot/privateDispose failure). Delegation input iteration stops at 12; primary disposal at 18; other disposals at 7. The test host's timer iterator is bounded at next five, with source-driven live-key witnesses; this is bounded coverage, not an exhaustive input claim. The independent committed-object census expects eleven file-qualified symbols, scans 26 manifests / 520 rows and stops at syncUnifiedBalanceUpnlFromPositions.ts.toml. Counts are not derived from a generated artifact's own length.

Initial read-only review found no functional mismatch or vacuous assertion and judged all eleven synchronous bodies appropriate for ported status. Final review verified eleven real targets/headers, nine public functions, census expectations and unique control selectors. Only the two note accuracy details above required correction; production/tests were unchanged. Reviewer ran the source-only 139-case oracle with exit 0 and ran no Cargo or controls. Queue completion check exited 0 with nothing outstanding; this task was not inserted in the queue.

Focused native command: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript "${CARGO_BIN}" test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test stream_delegation --jobs 2 -- --include-ignored`. Baseline and five exact restores at `926d8916f2b5932ff46c93c9c2aefca08e3d7a37` exited 0, each reporting one passed / zero failed / zero ignored and executing all 139 cases. Sequential control runner exited 0. All five compiling mutations failed at their named runtime assertion with 101; no invalid compile-only control or runtime survivor. Final working-tree source/tests/scripts restoration diff against this SHA exited 0.

| Family | Named native assertion | Mutation exit | Exact restore exit |
| --- | --- | --- | --- |
| single candle delegate | `STREAM native ohlcv event` | 101 | 0 |
| bulk candle delegate | `STREAM native bulk event` | 101 | 0 |
| order book delegate | `STREAM native book event` | 101 | 0 |
| socket disposal route | `STREAM native dispose event` | 101 | 0 |
| primary live timer key | `STREAM native dispose event primary timers` | 101 | 0 |

Tracked [runtime controls](evidence/stream-delegation-controls.json) and [committed-object census](evidence/stream-delegation-census.json) pin exact measured SHAs. Control floor is five, stopped at primary live timer key.

Not run: full Bybit/venue suite, workspace/monorepo builds or tests, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live exchange/network calls or full exchange/socket/constructor/host composition. No shared type or trait changed. No PR was opened or merged by this agent.

Fresh extractor check at `926d8916f2b5932ff46c93c9c2aefca08e3d7a37` after the final note corrections exited 0: 7341 symbols / 512 files, zero drift/additions/orphans. Output text was inspected; the Yarn exit alone was not the criterion.

## Stream delegation merge retention — a42c40d5e813156927a90c3c8e4ad529c2e4a9e0

Fetched and merged integration `95b56a79bc646156e1812b17428e0fb6295413a5` down cleanly. Separate retention `git diff --exit-code` measurements against `926d8916f2b5932ff46c93c9c2aefca08e3d7a37` exited 0 for whole Bybit src/tests/scripts/Cargo manifest/parity manifests, and 0 for shared root Cargo/lock/config, num/types/exchange/transport/store/crypto/candle-cache directories, maintained Bybit oracle source and extractor inputs. The inspected local package floor is eight including Bybit, stopped at safe-cex-venue-bybit in sorted order. Byte-unchanged case applies: scoped all-targets check remains measured at `d757187187674f71c1b38b0802c5567545a49be2`, native baseline and five named runtime mutations/restores at `926d8916f2b5932ff46c93c9c2aefca08e3d7a37`. These gates were not rerun or relabeled at the merge.

Fresh extractor check at `a42c40d5e813156927a90c3c8e4ad529c2e4a9e0` exited 0, with 7341 symbols / 512 files and zero drift/additions/orphans. Fresh committed-object tomllib census against pinned integration `95b56a79bc646156e1812b17428e0fb6295413a5` exited 0: exactly eleven pending->ported / zero pending->divergent; totals 154 ported / 125 divergent / 241 pending from 143 / 125 / 252. The tracked census pins measured/base SHAs and the original reviewed integration parent `648fe27409`. This following evidence-only commit leaves measured gate inputs unchanged. Full venue/workspace/monorepo suites, host/wasm Clippy, Rust indexer, CI rerun, live network and full exchange/socket/constructor composition were not run. No PR was opened or merged by this agent.

## Ticker readers — eleven maintained primary/inverse/spot bodies

Fresh branch `feat/bybit-ticker-readers` starts directly at reviewed integration `0e116b60e90e37e08f3995645493fc20176c0b86`, with no unreviewed feature branch ancestor at creation. Initial status/log found only the two untracked task instruction files, with no inherited code or broken mutation. `.aimux-next.md` was read; CLAUDE.md and agent-memory were not reread.

Eleven pending rows have six real public function targets in ticker_readers.rs: primary/inverse fetchTickersWithoutParse, fetchTickers, fetchTickersRaw, fetchTickersRawHelper, and the first three spot readers. All eleven are ported on their maintained dependencies and retained transport-value domain. Factories start eagerly, including before first poll; wrappers await raw before disposal, return a fresh empty array or original store/raw/parse values as source dictates. Helpers preserve strict get-key status aliases, message logging, [null] failure sentinel, original successful list identity and missing/null/nonarray values plus access errors. Primary checks only null elements; inverse reads categories before one helper binding, starts all indexed requests eagerly (including binding for empty categories), awaits host Promise.all, performs input-order spread with string codepoints, and checks null. Array/object handles are mutable Rc<RefCell> allocations, including flattened element identity and alias mutations from disposal/parse callbacks. Executable prototypes/accessors and full class/transport/scheduler construction remain outside these maintained dependency bodies; no general JavaScript emulator or microtask-parity claim is made.

| Measurement | Exact measured SHA | Exit / observed output |
| --- | --- | --- |
| Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2` | `7772041b77f9048b6970d7575544d7660f26920a` | 0, Finished dev profile |
| Extractor check via explicit Node/Yarn command | `7772041b77f9048b6970d7575544d7660f26920a` | 0; 7341 symbols / 512 files; zero drift/additions/orphans |
| Committed-object tomllib census | `7772041b77f9048b6970d7575544d7660f26920a` | 0; 11 pending->ported / 0 pending->divergent; 165 ported / 125 divergent / 230 pending from 154 / 125 / 241 |

Extractor writer ran after manifest edits and reported three updated manifests, zero added/orphaned symbols. Output text was inspected rather than trusting Yarn's exit alone. Independent file-qualified census expects eleven exact targets/statuses, scans 26 manifests / 520 rows and stops at syncUnifiedBalanceUpnlFromPositions.ts.toml. Initial census/extractor at `1c909aa2b97317513b6a355e088eca5b58835f9f` also exited 0 with the same rows/counts; the later SHA adds only the caller-settlement test-host correction.

The oracle uniquely extracts and evaluates the maintained class members, uses maintained get-key and ENDPOINTS declarations, and replaces only explicit dependencies. Constructors are excluded. Independent case floors: three variants x fourteen without-parse and fourteen parsed cases (84), three variants x nineteen helper cases (57), primary raw thirteen, inverse raw eighteen; total 172, stopped at iteration 171 (inverse emit failure). Per-group inputs stop at 13 / 18 / 12 / 17 respectively. Native future and sibling-drain bounds are sixteen polls; source drain is sixteen microtasks. Pre-first-poll calls, settlement events before full sibling drain, native enum variants/fields/f64, return/store/list/flattened-row pointer identities and original host error identity are asserted. The rejection discriminant precedes result fields. No native serialization/re-encoding/roundtrip or wire assertion is used.

Review corrected two proof boundaries before release measurement: immutable allocations became mutable shared handles with source/native alias witnesses; final-only sibling events became a separate settlement snapshot. The original delay1/3 source case had already completed both siblings at settlement and was non-discriminating for first rejection; delay1/8 now records only helper0 at settlement and both helpers after drain. Native host tasks remain owned independently of join rejection. Two fixed host turns model bounded caller continuation/rejection jobs before the settlement snapshot, independent of expected event count; this also covers a synchronous all-factory error whose sibling completes during source promise propagation. The additional wait-all mutation below must detect regression of this contract. Read-only source/state-model review exited 0 and confirmed the revised distinction, eleven real mappings and no remaining concrete body finding on the declared boundary. Reviewer ran no Cargo or mutations.

Development observations, not release gates: initial test compile exited 101 for a borrowed helper binding; moved binding/host capture fixed it. Mutable-adapter edits initially caused two expected-JSON borrow method compile errors (101); fixed without changing production. Named settlement assertions exited 101 first for delay1/3's inadequate source separation, then for the all-factory caller-continuation model. These findings were corrected without dropping any settlement assertion. The 160-case development baseline and final 172-case development run each exited 0; neither dirty-tree run is relabeled as a release gate.

Two compiler attempts were stopped with owned-rustc SIGTERM, and Cargo exited 101: the first dirty-tree attempt was atop `0e116b60e9` and ended after commit `1c909aa2b9`; the second began at `1c909aa2b9`. Sampling showed macOS dyld blocked in fcntl while loading a procedural-macro library, with 0% CPU; neither is a runtime discrimination or passed gate. Only the owned target-cache auto_impl dylib inode was refreshed using a byte-identical copy (SHA256 af2a5ad29a72b67f2e2313c00751cb077cbc71faa779f1f5cd8741793003cda4); signature verification exited 0. Compiler recovered. Source/dependency code was not changed by this cache operation. At `1c909aa2b9`, the recovered run compiled but exited 101 at the named caller-settlement development assertion; after its nine-line host correction, the final development test exited 0 and release measurements use `7772041b77`.

At `7772041b77f9048b6970d7575544d7660f26920a`, the initial control runner exited 1: without-disposal mutation failed at its named native return assertion (101) and exact restore passed (0); parsed-disposal also failed at a native settlement assertion (101) and restored (0), but the runner expected a later parsed-event assertion. This is a selector finding, not a runtime survivor. The [retained selector finding](evidence/ticker-readers-selector-finding.json) records both attempted controls and the measured SHA. Commit `439136ec63af7e425426b0cc260473bc3436f5ef` corrects parsed/helper selectors to the earlier settlement assertion. A separate source/tests/Cargo retention diff from `7772041b77` to `439136ec63` exited 0. Fresh census at `439136ec63` exited 0 with eleven pending->ported, zero divergent moves and 230 pending. Fresh extractor check at that SHA exited 0 with 7341 symbols / 512 files, zero drift/additions/orphans; output text inspected.

Not run: full Bybit/venue suite, workspace or monorepo builds/tests, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live exchange/network requests, or full class/constructor/transport/scheduler composition. No shared type or trait changed. No PR was opened or merged by this agent.

During the final control run at `439136ec63`, sampling the owned second-restore rustc again showed dyld procedural-macro loading blocked in fcntl. This attempt was allowed to progress without termination or further cache changes; the restore subsequently exited 0. The delay itself is not a gate failure or runtime survivor.

Focused native command: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript "${CARGO_BIN}" test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test ticker_readers --jobs 2 -- --include-ignored`. Baseline and all six exact restores at `439136ec63af7e425426b0cc260473bc3436f5ef` exited 0, each reporting one passed / zero failed / zero ignored and running all 172 cases. The sequential runner exited 0. All six compiling mutations failed at the named native runtime assertion with 101. No runtime survivor in this final six-control floor, stopped on host all first rejection. The earlier selector finding remains recorded above rather than erased.

| Family | Named native assertion | Mutation exit | Exact restore exit |
| --- | --- | --- | --- |
| without disposal | `TICKER native without return` | 101 | 0 |
| parsed disposal | `TICKER native settlement` | 101 | 0 |
| primary category | `TICKER native primaryRaw eager calls` | 101 | 0 |
| inverse flatten order | `TICKER native inverseRaw return` | 101 | 0 |
| helper strict status | `TICKER native settlement` | 101 | 0 |
| host all first rejection | `TICKER native settlement` | 101 | 0 |

Tracked [runtime controls](evidence/ticker-readers-controls.json) and [committed-object census](evidence/ticker-readers-census.json) pin exact measured SHAs. Final source/tests/scripts/Cargo/parity restoration diff against `439136ec63` exited 0; whitespace check exited 0. Queue completion check exited 0 with nothing outstanding; this task was not inserted into the queue. This following evidence-only commit preserves the measured gate inputs, and is not relabeled as a Cargo/test measurement.

## Ticker readers merge retention — 10e7fa58c1f9aefd412d6b3ca89f40b6af56d17f

Fetched and merged reviewed integration `7e9e5c660472d985e231394357b25ed171b929f2` down cleanly before push. Fresh branch parent remains `0e116b60e90e37e08f3995645493fc20176c0b86`; no unrelated unreviewed feature branch was used as an ancestor. Separate retention diffs against `439136ec63af7e425426b0cc260473bc3436f5ef` exited 0 for whole Bybit src/tests/scripts/Cargo/parity manifests and 0 for inspected shared root Cargo/config, num/types/exchange/transport/store/crypto/candle-cache directories, maintained Bybit/get-key oracle inputs and extractor inputs. A manifest-driven traversal also exited 0 for Cargo/src/tests/examples/benches/build.rs inputs in eight local packages: candle-cache, crypto, exchange, num, store, transport, types and Bybit, stopped at safe-cex-venue-bybit in sorted order.

Cargo.lock changed elsewhere. A dependency-graph traversal from the Bybit lock entry compared every reachable package record across the measured gate SHA and this merge and exited 0: floor 481 unchanged records, stopped at zmij 1.0.23 in sorted order. These are retention measurements, not workspace compilation. Byte-unchanged gate-input case applies: scoped all-targets check remains measured at `7772041b77f9048b6970d7575544d7660f26920a`; focused native baseline and six runtime mutations/restores remain measured at `439136ec63af7e425426b0cc260473bc3436f5ef`. Cargo and controls were not rerun or relabeled at this merge.

Fresh extractor check at `10e7fa58c1f9aefd412d6b3ca89f40b6af56d17f` exited 0: 7341 symbols / 512 files, zero drift/additions/orphans; output text inspected. Fresh committed-object tomllib census against pinned integration `7e9e5c660472d985e231394357b25ed171b929f2` exited 0: exactly eleven pending->ported / zero pending->divergent; totals 165 ported / 125 divergent / 230 pending from 154 / 125 / 241. Manifest floor is 26 files / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml. The tracked census pins both exact SHAs and original fresh parent. This following evidence-only commit preserves measured gate inputs. Full venue/workspace/monorepo suites, host/wasm Clippy, standalone Rust indexer, CI rerun, live network and full class/constructor/transport/scheduler composition were not run. No PR was opened or merged by this agent.

## Primary order lifecycle — ten synchronous primary bodies

Fresh branch `feat/bybit-primary-order-lifecycle` starts directly at reviewed integration `f6c865ddd8331c32516f063d90d045761bd2c73b`, with no unreviewed feature ancestor. Initial status/log found only untracked task instructions, no inherited source or deliberate mutation. `.aimux-next.md` and priority rules were read; `.aimux-brief.md` was absent. CLAUDE.md and agent-memory were not reread.

User's subsequent hard partition superseded the uncommitted multi-variant balance selection. Six inverse/spot/Hyro balance rows were skipped, and the balance work was saved outside the repository without landing any row. Its one dirty-tree Cargo attempt atop `157f83240a85280c25df395ba6a51225813fe9bf` exited 101 for an emitter callback arity typo; no balance Cargo gate passed or is claimed. The current slice changes only the primary exchange manifest and new native primary lifecycle module/tests/scripts, plus module registration. It does not modify the other lane's source/manifests.

Ten pending rows now have ten real function targets in primary_order_lifecycle.rs: demo/site fallback, mark seen, timer clearing, reduce-only predicate, timer scheduling, filled target, record, retirement, symbol resolution and snapshot preservation. All ten are ported on decoded Order DTOs and explicit host dependencies. Mutable order/list/timer/cache-entry identity, insertion-ordered maps with deletion/reinsertion, live iteration, callback ordering/short circuit and original callback errors are retained. Timer callbacks read the current reconciliation method before the captured mutable order's current ID and discard its result. Reconciliation execution/ownership belongs to the called dependency; the async reconcile row remains pending. Retirement forwards actual fresh mutable native `{id}` records in a fresh mutable list, not reconstructed test output.

Read-only review caught two body-adapter gaps before committed gates: retirement initially passed strings rather than id records; these became real native records with direct ID/distinct-handle assertions. Recent map values initially cloned entries before a store callback; they now retain Rc<RefCell<RecentEntry>>, with a lazy read of entry.order after absent store lookup. The in-place entry.order replacement witness returns the original ReplacementA handle. The original 78-iteration dirty native run exited 0; the expanded 79-iteration dirty run also exited 0. Neither is relabeled as a committed gate. Final read-only review found ten real targets/headers and no remaining concrete finding on the stated boundary; reviewer ran source-only oracle probes (0), no Cargo or runtime controls.

The exact maintained primary class members and three delay/grace/TTL declarations are selected uniquely by AST and executed with explicit clock/timer/store/reconciliation hosts; maintained Order enums and Brazil site ID are loaded. Full exchange construction is excluded. Independent floors: 12 fallback cases (stop 11), 8 reduce-only cases (stop 7), 8 target cases (stop 7), 12 scenario programs (stop 11) containing 51 steps. Total native iterations 79, stopped at global 78, program 11 step 4 (unknown symbol/store rejection). Timer clearing captures initial array length while reading live slots; witnesses include append/splice/map-list replacement. Scenarios cover mutable order ID changes during clock/setter/method getter, first-store and original row/list/entry identity, live recent-map insertion and captured entry replacement, grace/TTL equality, NaN/infinity/signed zero and original error identity. Rejection discriminants precede result fields. Native fields, f64 values, event arguments and pointers are asserted directly; no native serialization/re-encoding/roundtrip or wire assertion is used.

Extractor writer after the sole manifest edit reported one manifest updated and zero additions/orphans. Fresh extractor check at `1fb8efb5585767e828de31dcb9eb526ea20dfde5` exited 0: 7341 symbols / 512 files, zero drift/additions/orphans; output text inspected. Committed-object tomllib census at the same SHA against the fresh parent exited 0: exactly ten pending->ported / zero pending->divergent, Bybit totals 175 ported / 125 divergent / 220 pending from 165 / 125 / 230. Independent expected targets/statuses are file-qualified; floor 26 manifests / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml. A separate owned-partition tomllib enumeration exited 0 at the same SHA: six owned files, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml; owned pending 81->71, primary exchange 58->48. Separate excluded-manifest/source retention diff against the fresh parent exited 0.

Not run: full Bybit/venue suite, workspace/monorepo builds or tests, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live exchange/network calls or full exchange/constructor/socket/reconciliation dependency composition. No shared type or trait changed. No PR was opened or merged by this agent.

Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2` at `1fb8efb5585767e828de31dcb9eb526ea20dfde5` exited 0, Finished dev profile in 3m09. This is a crate-scoped all-targets check, not workspace compilation.

Focused native command: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript "${CARGO_BIN}" test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test primary_order_lifecycle --jobs 2 -- --include-ignored`. Baseline and all six exact restores at `1fb8efb5585767e828de31dcb9eb526ea20dfde5` exited 0, each reporting one passed / zero failed / zero ignored and running all 79 iterations. Sequential control runner exited 0. Every compiling mutation failed at its named native runtime assertion with 101; no runtime survivor or invalid compile-only control in this final floor. Six controls, stopped on captured mutable entry.

| Family | Named native assertion | Mutation exit | Exact restore exit |
| --- | --- | --- | --- |
| demo fallback | `PRIMARY native fallback` | 101 | 0 |
| timer lifecycle | `PRIMARY native number timer delay` | 101 | 0 |
| filled target | `PRIMARY native number target` | 101 | 0 |
| recent record | `PRIMARY native cases case 1 step 0 record: event count` | 101 | 0 |
| retired symbol TTL | `PRIMARY native cases case 7 step 2 resolve: absent symbol` | 101 | 0 |
| captured mutable entry | `PRIMARY native cases case 10 step 2 preserve: original preserved order identity` | 101 | 0 |

Tracked [runtime controls](evidence/primary-order-lifecycle-controls.json) and [committed-object census](evidence/primary-order-lifecycle-census.json) pin measured SHAs. Source/tests/scripts/Cargo/primary-manifest restoration diff against the measured SHA exited 0. Queue completion check exited 0 with nothing outstanding; the task was not inserted into the queue. This following evidence-only commit preserves all measured gate inputs, without relabeling their SHA.

## Primary lifecycle merge-down — 6ac01627dd76dcd60d606e85510b63ef75119022

Fetched and merged reviewed integration `3cc94529835dde67ba3e6af513a233baa307c3f0` down cleanly. Fresh parent remains `f6c865ddd8331c32516f063d90d045761bd2c73b`; no unrelated unreviewed feature branch was used. Scoped `git diff --exit-code 1fb8efb558... HEAD` exited 0 for whole Bybit src/tests/scripts/Cargo and the sole primary manifest. A separate diff exited 0 for root Cargo/config, num/types/exchange/transport/store/candle-cache, maintained Bybit/types/logging/interfaces/math oracle sources and extractor. Source for shared crypto changed (poseidon loader/example and module registration), so this is not a claim that all compile inputs remained byte-unchanged.

The Bybit code/fixture/control inputs survived unchanged: six named runtime mutations/restores remain measured at `1fb8efb5585767e828de31dcb9eb526ea20dfde5`, without relabeling. After shared crypto changed, fresh scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2` at `6ac01627dd76dcd60d606e85510b63ef75119022` exited 0, Finished dev profile in 1m16. A Bybit lock dependency-graph audit also exited 0: all 481 reachable package records remained identical, stopped at zmij 1.0.23 in sorted order; this is retention evidence, not a build.

Fresh extractor check at `6ac01627dd76dcd60d606e85510b63ef75119022` exited 0: 7341 symbols / 512 files, zero drift/additions/orphans; output text inspected. Fresh committed-object tomllib census against pinned integration `3cc94529835dde67ba3e6af513a233baa307c3f0` exited 0: ten pending->ported / zero pending->divergent; 175 ported / 125 divergent / 220 pending from 165 / 125 / 230. Floor 26 manifests / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml. The tracked census pins exact measured/base SHAs and original fresh parent.

Fresh focused native baseline at `6ac01627dd76dcd60d606e85510b63ef75119022` exited 0 after the changed crypto dependency compiled: one passed / zero failed / zero ignored, all 79 iterations executed. This is the new baseline measurement; the six unchanged-body runtime control results remain pinned to `1fb8efb558`, not relabeled. The following evidence-only commit preserves the new baseline/check/extractor/census inputs. Full venue/workspace/monorepo suites, host/wasm Clippy, standalone Rust indexer, CI rerun, live network and full exchange/constructor/socket/reconciliation dependency composition were not run. No shared type or trait was changed by this slice, and no PR was opened or merged by this agent.


### Public market refresh and OHLCV recovery — ten pending rows

Fresh branch `feat/bybit-public-data-recovery` starts directly from integration `b82816a58e8f5408f7534c659e13d29eea4dbdad`. No unrelated unreviewed feature branch is an ancestor. This slice changes only the primary exchange and public websocket manifests: three market/fee/retry readers and seven public open/timer/recovery/market-change bodies. All ten have real native targets and are ported; zero divergent rows added. Timer operations are explicit cancellable host dependencies, not generation-guarded sleeper replacements; full socket/core timer registration and cancellation composition remains outside this body proof.

| Measured gate | Actual measured SHA | Exit/result |
| --- | --- | --- |
| Scoped Bybit `cargo check --all-targets --jobs 2` | `2b93ce0c7bcca56c4452f4750fb4cad788bed238` | 0; Finished dev profile in 2m29 |
| Focused `public_data_recovery` native/source-oracle test with required TypeScript env | `2b93ce0c7bcca56c4452f4750fb4cad788bed238` | 0; 1 passed / 0 failed / 0 ignored; 81 scenarios |
| Control runner native baseline | `8c3f42fee4fff828b3cf6fa5a03e4266bfd65df3` | 0; same focused test |
| Ten deliberate method controls and exact restorations | `8c3f42fee4fff828b3cf6fa5a03e4266bfd65df3` | runner 0; every named runtime assertion 101; every restored test 0 |
| Explicit Node/Yarn extractor check | `8e915ff872248746c185835d98075605979bb514` | 0; 7341 symbols / 512 files; zero drift/additions/orphans; output text inspected |
| Committed-object tomllib census against fresh parent | `8c3f42fee4fff828b3cf6fa5a03e4266bfd65df3` | 0; 10 pending→ported / 0 pending→divergent; 175/125/220 → 185/125/210 ported/divergent/pending |
| Read-only sub-agent source oracle, targeted null/proxy/eager checks, independent ten-target mapping | `8c3f42fee4fff828b3cf6fa5a03e4266bfd65df3` | each 0; no remaining concrete finding; reviewer ran no Cargo/native tests/extractor/mutations |

Exact command/exits and measured SHAs are in tracked `evidence/public-data-recovery-gates.json`, `evidence/public-data-recovery-controls.json`, and `evidence/public-data-recovery-census.json`. The control-only commit after `2b93ce0c7b` changed no source/test/oracle bytes: scoped diff exited 0. Extractor TS/manifests/logging inputs from `8e915ff872` also survived to the release head unchanged (diff 0). These are retention statements, not relabeled executions. Extractor writer ran after manifest changes: two updated manifests, zero added/orphaned symbols. Rust file headers were derived from actual targets.

The oracle uniquely extracts the ten maintained class members and maintained ENDPOINTS, INTERVAL, severity and timeout/retry declarations, excluding constructors. Hosts are explicit clocks, timer operations, emitter/store access, request factories, retry predicate/description/error construction, logging, sleep and transport boundaries. Native DTO fields/discriminants, f64, callback ordering, eager pre-first-poll effects, receiver replacements, fresh ticker allocation, successful list/reply/cache identity, original branded/error/cause identity, and diagnostic payload/args identity are asserted. All native assertions and independent counts finish before any wire string comparison; frame operations/topics are decoded first. No decode/re-encode or roundtrip comparison is used.

Independent scenario floors: open 8, clear 8, arm 9, existing timers 3, recovery wrapper 5, recovery topics 12, markets changed 11, fees 7, markets 8, retry 10; total 81, stopped at iteration 80 (second-request rejection). Native futures are bounded by sixteen polls; no arbitrary microtask-timing equivalence is claimed. Census floor: 26 manifests / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml. Control floor: ten, stopped at market retry delay. Each control keeps its named assertion and measured failure/restore; no survivor or invalid runtime control was dropped.

Read-only review corrected receiver capture before category/setTimeout/handler/ticker getters, symbol reread after resolving the recovery method, and synchronous request throws entering catch before first poll. It also required retained branded-error pointer assertions and moving wire comparisons after every native assertion. Development native exits 101 exposed two oracle-host bugs: null timer/data defaulting via `??` (clear iteration 10) and duplicate Proxy/accessor assignment observations (markets iteration 50); both were fixed and the full 81-case baseline then passed. Initial source-oracle setup exited 1 because Timeframe is a type alias rather than an enum; the loader now extracts the maintained logging enum and interval declaration. These failed development attempts are not release gates.

Compiler stalls were observed in directory reading and dyld fcntl macro loading. Two worktree-local ignored proc-macro cache files were refreshed atomically with identical SHA-256 content before/after; hashes and paths are recorded. No production/dependency bytes or another lane's cache were changed, and no running gate was terminated.

Not run: full venue tests, workspace compilation, monorepo suites/builds, host/wasm Clippy, standalone Rust indexer, CI rerun, live network, or full exchange/constructor/socket/timer/XHR dependency composition. No shared type or trait was changed by this slice; no PR was opened or merged by this agent. Post-merge measurements and survival evidence follow below.
## HyroTrader private socket session lifecycle

Rows moved: **9 pending -> ported, 0 -> divergent, 0 regression** against integration `eb5339fff8f489456695778fe2af9701f8c18b69`. All Bybit: `175/125/220` -> `184/125/211` (ported/divergent/pending). Hard ten-file partition: `46/63/149` -> `55/63/140`. Independent Python 3.14 tomllib keys are (manifest path, TS symbol), floor 26 manifests / 520 rows, stopped at `sdk-rs/parity/manifest/exchanges/bybit/syncUnifiedBalanceUpnlFromPositions.ts.toml`. Only the nine rows in `hyrotrader.ws-private.ts.toml` moved. Plain Bybit exchange, sockets, income, API and unified balance helper remain with the other lane.

Fresh branch parent: `f6c865ddd8331c32516f063d90d045761bd2c73b`. Six fresh scoped gate exits below belong to **`c5c29fe276fe33d8814f758b53c06e7d753c1ca7`**. The completed native-control runner belongs to **`eb775c9aeca42d47a7e31e758b8d9f11efa90056`**; its native inputs were proven byte-unchanged at the correction SHA. Neither measurement is relabeled at the delivery/evidence commit. No unrelated unreviewed feature branch is an ancestor.

The owned unit is HyroPrivateWebsocket class allocation, constructor, sessionVersion, isRecoveryComplete, resetSession, resyncsPrivateStateOnReopen, sendReconnectSequence, request, and dispose. The generic value/error capability boundary retains UTF-16 code units, binary64 values, lexical callback receivers, live Map iteration, original error identity and Promise settlement. Original base construction/methods, nominal prototype, runtime property/coercion/Promise/Map/timer intrinsics, JSON parser/stringifier, HMAC, remote signer, clock, ID generator and reconciliation replies are explicit dependencies; their bodies are not claimed by this slice. The new SessionHost trait is venue-local. No shared SDK type/trait was changed.

The original whole TS class declaration is loaded by AST and transpiled without source-body edits. The source oracle executes 442 declared cases: class 4, accessors 5, reset 20, dispose 3, wrappers 126, requests 20, reconnect 264; stopped at reconnect/socket-reconcile/reconnect#0. Native assertions include actual field values, nominal prototype Rc identity, pairwise fresh Maps/functions, actual lexical callback receiver, getter binary64 bits, reset effects and live rejection count, pending acknowledgement state, timer delay/active state, error identity, decoded native request op/ID/args (including negative zero before stringify), auth key/expiry/signature and subscription topics, and readiness/reconciliation limits. Every supplemental definition/dependency/wire comparison is deferred until **all** native cases finish. Native dependency waits yield Pending once; the harness stops after 64 polls. This is a fixture floor, not exhaustive runtime or network proof. Standard field descriptor attributes are the declared define capability contract; no additional concrete host descriptor mutation is claimed.

| Gate | Exit | Measured SHA |
| --- | --- | --- |
| scoped-cargo-check | 0 | `c5c29fe276fe33d8814f758b53c06e7d753c1ca7` |
| hyro-native-controls | 0 | `eb775c9aeca42d47a7e31e758b8d9f11efa90056` |
| extract-check | 0 | `c5c29fe276fe33d8814f758b53c06e7d753c1ca7` |
| bybit-structure | 0 | `c5c29fe276fe33d8814f758b53c06e7d753c1ca7` |
| variant-divergence-notes | 0 | `c5c29fe276fe33d8814f758b53c06e7d753c1ca7` |
| scoped-rustfmt | 0 | `c5c29fe276fe33d8814f758b53c06e7d753c1ca7` |
| diff-check | 0 | `c5c29fe276fe33d8814f758b53c06e7d753c1ca7` |

Native-control retention at the correction SHA: exit 0 for the following exact input proof; the original control command was retained, not rerun:

`git diff --exit-code eb775c9aeca42d47a7e31e758b8d9f11efa90056 c5c29fe276fe33d8814f758b53c06e7d753c1ca7 -- sdk-rs/safe-cex-venue-bybit/src sdk-rs/safe-cex-venue-bybit/tests sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-session-fixtures.cjs sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-session-controls.py packages/safe-cex/src/exchanges/bybit/hyrotrader.ws-private.ts sdk-rs/Cargo.lock sdk-rs/Cargo.toml sdk-rs/safe-cex-venue-bybit/Cargo.toml sdk-rs/safe-cex-num sdk-rs/safe-cex-types sdk-rs/safe-cex-exchange sdk-rs/safe-cex-transport sdk-rs/safe-cex-store sdk-rs/safe-cex-crypto`

Exact measured commands (native controls executed at their original SHA):

`cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-session-controls.py`
`node "${YARN_CLI}" parity:extract:check`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-session-structure.py`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-session-notes.py`
`rustfmt --edition 2024 --check sdk-rs/safe-cex-venue-bybit/src/hyro_private_session.rs sdk-rs/safe-cex-venue-bybit/tests/hyro_private_session.rs`
`git diff --check`

Native controls at `eb775c9aeca42d47a7e31e758b8d9f11efa90056`. Scoped native target: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test hyro_private_session --jobs 2`. Baseline 0 and nine exact restore runs 0; one passed / zero failed / zero ignored, executing the full 442-case floor on each pass. The nine mutated production builds compiled and exited 101 at the required named native assertion. The runner verified unique edit sites and its Python AST before execution.

| Control | Required native assertion | Runtime mutation | Exact restore |
| --- | --- | --- | --- |
| class-ready | `class_native_initial_ready` | 101 | 0 |
| session-version | `version_native_generation` | 101 | 0 |
| recovery-complete | `recovery_native_complete` | 101 | 0 |
| constructor-observer | `constructor_native_order_observer` | 101 | 0 |
| reset-ready | `reset_native_ready_false` | 101 | 0 |
| resync-getter | `resync_native_false` | 101 | 0 |
| reconnect-ready | `reconnect_native_ready` | 101 | 0 |
| request-timeout | `request_native_timeout_delay` | 101 | 0 |
| dispose-base | `dispose_native_base_effect` | 101 | 0 |

No survivors among the nine completed controls attempted; stopped at dispose-base. Compiler rejects and a 101 from an unrelated site are not included in that claim. Each runner mutation restores the exact production bytes in finally; restored passing exits are separate measurements.

The authoritative parity:extract WRITER ran after manifest preparation and exited 0 (output: updated 0 manifests, 0 added, 0 orphaned). This pre-commit writer measurement is not attributed to the later gate SHA; the committed extractor-check is the SHA-bound gate above. The strict unit structure gate invokes the real parity checker over the original module/export and nine-row manifest: all 42 new public Rust items are checked, including all 29 associated host-contract items, with zero exemptions. Dependencies link to original SDK trees for name resolution. This is a new-unit structural claim, not a green whole-Bybit structural claim. The notes gate covers the ten assigned variant manifests and 63 canonical divergent notes; it excludes plain Bybit. Rustfmt covers the two new Rust files; inherited unrelated formatting is not a new claim.

SOURCE BEHAVIOR PRESERVED: connect marks started before reset; stale close/message sockets are ignored using the source truthy currentTarget condition; order observation precedes captured base handling; ping/base-message forwarding is readiness-gated while pre-ready topic frames still increment revision. Acknowledgements match both ID and op, delete the entry before clearing its timer and settling, and require success === true; numeric 1 rejects. Reset walks the originally obtained live iterator (visiting additions), then clears the current Map, and only then calls optional health sync. Partial reset/health/callback failures retain prior effects and original errors. There is no early disposed guard before reconnect reset/signing; guards occur at the source sites after signing/auth and during reconciliation. Recovery stops after three attempts; readiness is committed before track and ping. A current failure is normalized/logged before reset and an unawaited reconnect; a stale failure is suppressed. Timer/sending failures preserve source registration/cleanup/settlement ordering. None of these existing behaviours was silently tidied.

DISCLOSURES / WITHDRAWN MEASUREMENTS: the first census process used a Python without tomllib and exited 1; Python 3.14 supplied the actual census. A preparation helper initially had a wrong relative import and exited 1 with ERR_MODULE_NOT_FOUND; a writer that ran before that helper succeeded is not proof of updated rows. The helper path was corrected, metadata prepared through parseManifest/serializeManifest, and the writer rerun. Before the committed gates, the source observer used instanceof Map across VM realms and let constructing a second instance overwrite its observed first instance; these observer errors were corrected using a brand check and a retained first reference. The first uncommitted Rust test compile exited 101 with E0277 Vec<Value>/Value comparisons and E0308 Promise result conversion in the harness. These were instrument errors, not runtime controls, and are not attributed to the base tree. Production request executor review separately retained raw throws outside the send catch rather than normalizing them.

WITHDRAWN SEQUENCE at `eb01dc1d846254d12ad6c11a095eff245a0f966c`: review found supplemental comparisons were only deferred per case, so later cases could follow earlier wire comparisons. Scoped cargo check was intentionally interrupted (child -2 / runner 254) before any control ran. No green scoped check, completed control, restore test or seven-green-gate claim is made for that sequence. A separate test commit deferred every supplemental comparison until after all native cases, and the whole sequence above ran at the corrected SHA. Source bodies, fixture inputs/replies and production assertions were not weakened.

Merge-down measurement SHA: `451d0a7717fb03616953e97571ef9b46c78d8ab2`; fetched integration `eb5339fff8f489456695778fe2af9701f8c18b69`. Retention proofs against the gate SHA:

- required: exit 1; changed paths: `sdk-rs/safe-cex-venue-bybit/src/lib.rs`, `sdk-rs/safe-cex-venue-bybit/src/primary_order_lifecycle.rs`, `sdk-rs/safe-cex-venue-bybit/tests/primary_order_lifecycle.rs`.
- owned: exit 0; changed paths: none.
- expanded: exit 1; changed paths: `sdk-rs/parity/exempt.toml`, `sdk-rs/parity/manifest/exchanges/bybit/bybit.exchange.ts.toml`, `sdk-rs/safe-cex-venue-bybit/scripts/primary-order-lifecycle-census.py`, `sdk-rs/safe-cex-venue-bybit/scripts/primary-order-lifecycle-controls.py`, `sdk-rs/safe-cex-venue-bybit/scripts/primary-order-lifecycle-fixtures.cjs`.
- dependency: exit 1; changed paths: `sdk-rs/Cargo.lock`, `sdk-rs/safe-cex-crypto/POSEIDON_LOADER_GATE_EVIDENCE.md`, `sdk-rs/safe-cex-crypto/POSEIDON_LOADER_GATE_RESULTS.json`, `sdk-rs/safe-cex-crypto/src/lib.rs`, `sdk-rs/safe-cex-crypto/src/poseidon_example.rs`, `sdk-rs/safe-cex-crypto/src/poseidon_loaders.rs`, `sdk-rs/safe-cex-crypto/tests/poseidon_loaders.rs`, `sdk-rs/safe-cex-crypto/tests/poseidon_loaders_oracle.mjs`.

Reruns at **`451d0a7717fb03616953e97571ef9b46c78d8ab2`**, not the later evidence/delivery SHA:
- scoped-cargo-check: 0.
- hyro-native: 0.
- bybit-structure: 0.
- extract-check: 0.
- variant-notes: 0.

DELIVERY AUDIT: the first fetch exited 1 because Git reported refs/remotes/origin/feat/safe-cex-rust was already at eb5339fff8f489456695778fe2af9701f8c18b69 while its update expected 0ffe13d5ef70c19c137639eafaad6bc118fa6c9a. That attempt stopped before merge; no process responsible for the ref advance is claimed. A normal retry fetched successfully. Merge then conflicted only at src/lib.rs append exports; both hyro_private_session and the reviewed other-lane primary_order_lifecycle export were retained in the normal merge commit. No reset, ref rewrite, index/lock deletion or force push was used.


Not run: full Bybit crate/workspace/monorepo suites, workspace all-targets compilation, host/wasm Clippy, package TypeScript typecheck, live exchange/transport/credentials/network tests, or CI rerun. No PR was opened/merged by this lane. Original base class bodies, concrete SDK host integration and cancellation of unpolled/dropped Rust orchestration futures are not newly claimed. Final merge/push/remote SHA verification are delivery measurements, recorded separately; the evidence commit does not move the gate measurements.

Broad diagnostic attribution: the corrected broad comparison with the exact same 120 baseline diagnostics ran against the corrected **working tree before** the contract-map/script commit, subsequently committed as `c5c29fe276fe33d8814f758b53c06e7d753c1ca7`. It is not reported as a measurement executed at that commit or at the later merge SHA. The strict unit gate was actually rerun at both the correction SHA and `451d0a7717fb03616953e97571ef9b46c78d8ab2`; full Bybit structure was not rerun after merge.

### Delivery merge retention at 5ab3209469e997fe8709ff5081f7f413fdd10fcc

Fetched/merged integration `41f42dc2e11373cf24bceca1ac48f0310f468e99`. Native owned-input proof remained 0. Whole src/tests still differ from the correction SHA only by the previously reviewed primary lifecycle module/export; the delta from the already compiled/native-tested `451d0a7717fb03616953e97571ef9b46c78d8ab2` is 0. Dependency proof from that checked merge is 0. Expanded proof is 1 only for `sdk-rs/parity/exempt.toml`; conservative strict-unit structure, extractor check and variant notes reruns each exited 0 **at `5ab3209469e997fe8709ff5081f7f413fdd10fcc`**, not the later evidence or pushed SHA. Whole Bybit broad structure was not rerun. Fresh integration-relative census remains 175/125/220 -> 184/125/211; own partition 46/63/149 -> 55/63/140, exactly 9 pending->ported / 0 divergent / 0 regressions.

## Inverse and spot account refresh and polling

Rows moved: **9 pending -> ported, 0 -> divergent, 0 regression** against integration `6c27a13bdd9dc5645060283a5d74459405b7a0e2`. All Bybit `184/125/211` -> `193/125/202`; hard ten-file partition `55/63/140` -> `64/63/131` (ported/divergent/pending). Python 3.14 tomllib census keys (manifest path, TS symbol), 26 manifests / 520 rows, stopped at `sdk-rs/parity/manifest/exchanges/bybit/syncUnifiedBalanceUpnlFromPositions.ts.toml`. Only inverse/spot exchange manifests moved: fetchOrdersPositionsAndBalance, fetchAndSetPrivateSnapshot, fetchAndSetPrivateSnapshotHelper, tick in each, plus spot fetchPositions. Plain Bybit source/manifests were not edited.

Fresh parent `645f6fda62f3fab6787db45ab45688376008b739`. All seven scoped gate exits and all nine completed controls below were measured at **`5450ac69fe0c9aa0568ed5668e40e121fda933c5`**, not a later merge/evidence/delivery SHA. No unrelated unreviewed feature branch was used as an ancestor.

The native generic value/error boundary retains arbitrary original replies, UTF-16 text, binary64 values, opaque object/array/function identity, fresh loaded spreads, live self/store property lookup and fallible callbacks. AccountRefreshHost extends the existing venue-local SessionHost without changing it; original Promise then/all/object-spread capabilities provide the original scheduler/alias/getter contract, retain sibling work after aggregate rejection, and visibly observe detached rejection. Fetch/store/formatter/merge/clock/Math/loop dependency bodies and a concrete full exchange/SDK host are not newly ported by this slice. Dropped/unpolled native futures are not a new cancellation claim.

The unchanged original TS member bodies and source constants are extracted by AST (four inverse / five spot). Oracle floor **2772 cases**: inverse 1344 / spot 1428; 21 declared failure sites across fixed method/profile products, stopped at spot/fetchPositions/fresh/loop#0. Arrays/objects used as replies contain actual native payload fields; before any supplemental comparison the test asserts row counts/UTF-16 IDs, raw/formatted balance values, native alias identities, live store/loaded state, getters/clock counts, field write order, exact original errors, remaining pending requests, detached rejection, and binary64 timer/backoff values. Independent probes assert fresh mutable empty spot arrays and lexical loop receiver/no forwarded arguments/dynamic tick result identity. All supplemental eager/dependency traces are deferred until **all native cases finish**. The controlled original-Promise adapter yields on every await, stops after 20 request turns and 12 reaction turns, and preserves parent completion before later sibling publication.

| Scoped gate | Exit |
| --- | --- |
| scoped-cargo-check | 0 |
| native-controls | 0 |
| extract-check | 0 |
| unit-structure | 0 |
| partition-notes | 0 |
| scoped-rustfmt | 0 |
| diff-check | 0 |

Exact measured commands:

`cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/variant-account-refresh-controls.py`
`node "${YARN_CLI}" parity:extract:check`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/variant-account-refresh-structure.py`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-session-notes.py`
`rustfmt --edition 2024 --check sdk-rs/safe-cex-venue-bybit/src/variant_account_refresh.rs sdk-rs/safe-cex-venue-bybit/tests/variant_account_refresh.rs`
`git diff --check`

Native command: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test variant_account_refresh --jobs 2`. Baseline 0 and all nine exact restores 0 (one passed / zero failed / zero ignored, full 2772-case floor on each passing run). The runner verifies unique production function edit sites, restores exact bytes in finally, and requires compiled runtime 101 at the named site before counting a control.

| Control | Required native site | Compiled mutation | Exact restore |
| --- | --- | --- | --- |
| inverse-parallel-positions | `inverse_refresh_native_merge_window_read` | 101 | 0 |
| inverse-snapshot-wrapper | `inverse_wrapper_native_call` | 101 | 0 |
| inverse-sequential-positions | `inverse_helper_native_merge_window_read` | 101 | 0 |
| inverse-tick-reset | `inverse_tick_native_reset` | 101 | 0 |
| spot-parallel-positions | `spot_refresh_native_merge_window_read` | 101 | 0 |
| spot-snapshot-wrapper | `spot_wrapper_native_call` | 101 | 0 |
| spot-sequential-positions | `spot_helper_native_merge_window_read` | 101 | 0 |
| spot-tick-reset | `spot_tick_native_reset` | 101 | 0 |
| spot-empty-positions | `spot_positions_native_empty` | 101 | 0 |

No survivors among the nine completed controls attempted; stopped at spot-empty-positions. Compiler rejects and a 101 from an unrelated site are not included in that claim.

The authoritative parity:extract writer ran after metadata preparation: exit 0, updated 0 manifests / 0 added / 0 orphaned; this working-tree writer measurement is not relabeled at the later code SHA. The committed extractor-check above is SHA-bound. Strict new-unit structure uses original export/source/Cargo bytes, the nine actual rows and all **14 new public Rust items**, zero exemptions. The inherited broad Bybit structural RED reported in the preceding section is not claimed green and was not rerun; user explicitly classified those inherited findings as fleet noise. Notes gate covers the ten assigned manifests (63 canonical divergent notes). Rustfmt covers the two new Rust files.

SOURCE BEHAVIOR PRESERVED: parallel request factories run in source order and each then handler is attached before the next factory; synchronous later factory failure can leave an earlier derived promise detached. The aggregate settles at first rejection while siblings can still commit order/position state after the parent has logged and returned. Already-disposed parallel refresh still starts requests. Sequential helpers fetch before each post-await disposal guard, skip only orders for false/falsy flags, and treat explicit undefined as the default true. Inverse merges when now-last < 1000 strictly, including future timestamps and +Infinity (negative difference), while NaN does not merge; spot never reads that clock. Positions commit before the loaded flag update, and each loaded payload is freshly spread from live state. Spot fetchPositions returns a fresh mutable [] and ignores symbols. Tick clears needsReinit before refetch, reads Date.now separately for gate and lastReinitAt, rereads state after completion, doubles/caps retry only if still needed or resets 15000, and calls helper(false) even when disposal happened during refetch. Reinit failures are outside the helper catch and prevent loop scheduling; helper failures emit only err?.message (including undefined for a raw string) before the final disposal check. The loop arrow rereads current tick and returns its result. Original visible logging/partial effects are preserved; none was silently corrected.

INSTRUMENT / REVIEW DISCLOSURES BEFORE COMMITTED GATES: the initial source oracle exited on an unhandled rejection from the genuine detached-then source path. A visible process observer now captures original unhandled error identity/UTF-16 receipts; source bodies were unchanged. The first Rust preflight compiler 101 was an unused Future import under -D warnings, not a runtime control. The adapter initially omitted already-settled promise reaction registration and always-yield await behavior; those were repaired. An uncommitted runtime 101 at native mid-state inverse/parallel/positions-first/update#0 showed the driver settled an extra sibling before observing aggregate rejection; it was corrected to poll the parent before advancing siblings. Another uncommitted 101 at inverse_refresh_native_positions_identity exposed token() using a truthy literal condition: all declared timestamp profiles had become undefined. The generator was corrected to the declared profile values rather than weakening the expectation. Review separately corrected the owned loop callback to return the current tick result and the helper to apply the source undefined default. These were pre-commit findings, not controls or measurements attributed to the base tree.

REJECTED SEQUENCE at `2accdc3d181bfb48e843bef1879e49426c3860dc`: scoped all-targets check 0 and baseline native 0. The first inverse-parallel-positions mutation compiled and exited 101 at native mid-state inverse/parallel/old/clock#0 BEFORE its required merge-identity assertion; runner then exited 1. Production restored automatically in finally, but **no post-control restore test ran**. Zero controls are counted completed, and no seven-green-gate claim is made for that sequence. A separate commit added independently asserted merge-window clock reads before state comparisons (one for inverse old timestamp, zero for spot), retained the existing identity/state assertions, and moved those four control requirements to the earlier native read site. The entire seven-gate/nine-control sequence above was then rerun at the correction SHA.

Merge-down measurement at `63438431c28712eaafeb18f4e86e6dd84c50b426`, fetched integration `6c27a13bdd9dc5645060283a5d74459405b7a0e2`. Gate retention proofs:

- required: exit 0; changed none.
- owned: exit 0; changed none.
- expanded: exit 0; changed none.
- dependency: exit 1; changed `package.json`, `sdk-rs/Cargo.lock`, `sdk-rs/Cargo.toml`.

Reruns measured at **`63438431c28712eaafeb18f4e86e6dd84c50b426`**, not later delivery:
- scoped-cargo-check: 0.
- account-native: 0.
- unit-structure: 0.
- extract-check: 0.

Not run: full Bybit/workspace/monorepo suites, workspace compilation, host/wasm Clippy, package TS typecheck, broad Bybit structure rerun, live exchange/network/credentials tests, CI rerun, or full concrete class/host integration. No shared SDK type/trait was modified; no PR was opened/merged by this lane. Evidence is committed here; raw logs/corpus remain ignored. Final merge/push/remote verification are separate delivery measurements.

Merge dependency detail at `63438431c28712eaafeb18f4e86e6dd84c50b426`: Cargo.toml/Cargo.lock add `safe-cex-lighter-sdk` only; root package.json adds KuCoin `order_control` and `bootstrap_reads` oracle targets. Maintained source, Bybit scripts/manifests and shared types/num/exchange/transport/store/crypto/yarn.lock were unchanged. Scoped cargo/native, strict unit structure and extractor reruns each exited 0 at that merge SHA; the gate/control measurements remain at `5450ac69fe0c9aa0568ed5668e40e121fda933c5`.


### Public data recovery post-merge retention and refreshed gates

Merged integration `6c27a13bdd9dc5645060283a5d74459405b7a0e2` down; merge completion is `441e3fa59759ad53a6accab5aa717174672e4a26`. The first merge exited 1 for competing appended GATE_EVIDENCE sections; both were preserved and the merge completed at exit 0. Full crate source/tests diff from the control SHA exited 1 because integration added the other lane's HyroTrader session module/test and lib registration. The selected recovery source, native test, oracle/control scripts and two primary/public manifests diff exited 0. No other lane's implementation or manifest was edited to resolve this merge.

At `441e3fa59759ad53a6accab5aa717174672e4a26`, fresh scoped Bybit all-target check exited 0 (Finished dev profile 2m25), fresh focused public_data_recovery test exited 0 (one passed / zero failed / zero ignored; all 81 scenarios), fresh extractor check exited 0 (7341 symbols / 512 files, zero drift/additions/orphans; output text inspected), and fresh committed-object tomllib census exited 0 against pinned integration `6c27a13bdd`: ten pending→ported / zero pending→divergent; 184/125/211 → 194/125/201 ported/divergent/pending. Census floor remains 26 manifests / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml. Initial census figures above remain attached to their earlier SHA; these are the refreshed figures.

The ten method mutation/restoration measurements remain attached to `8c3f42fee4fff828b3cf6fa5a03e4266bfd65df3`, whose selected inputs survived unchanged; they were not relabeled as running at the merge SHA. The following evidence-only commit changes no gate source/test/oracle/manifest inputs. The named full-suite/workspace/Clippy/network/composition non-claims above still apply.


Public data recovery final refresh: integration `a1338bc368a19c179baf10f08fab602064136fc9` was merged down to `c96a2066c5d6db743f456340b107a25127ed7ebd` immediately before the successful push (exit 0); PR7740 then merged that slice. Bybit source/tests/scripts, Cargo/config and relevant shared crate/oracle inputs diff from the refreshed gate SHA `441e3fa597` exited 0. The added Crypto.com manifest work changed global parity inputs (diff 1), so a fresh extractor check at `c96a2066c5` exited 0 (7341 symbols / 512 files; zero drift/additions/orphans; output text inspected), and a fresh committed-object tomllib census against `a1338bc368` exited 0 (exactly ten pending→ported / zero→divergent; 194/125/201 after). The check/test exits remain pinned to `441e3fa597`, controls to `8c3f42fee4`; none was relabeled. These final evidence updates are carried on the next fresh branch from `9fad5d8d1bbb15b743af7af172cbcc5c69238a5a` because the prior slice merged before the refresh evidence was committed; no further commits are pushed to its merged branch.


## Primary private snapshot and readers

Rows moved: **10 pending -> ported, 0 -> divergent** at feature SHA `c74fdaa01020cff794f85f055dd25747d54b380a`, against fresh integration parent `9fad5d8d1bbb15b743af7af172cbcc5c69238a5a`. Independent committed-object Python 3.11 tomllib census: Bybit 194/125/201 -> 204/125/191 (ported/divergent/pending), floor 26 manifests / 520 rows, stopped at `syncUnifiedBalanceUpnlFromPositions.ts.toml`. Only ten primary `bybit.exchange.ts.toml` symbols changed; inverse/spot/demo/HyroTrader source and rows were not edited. The earlier recovery evidence-only commit is inherited between the reviewed integration parent and this feature; there is no unrelated unreviewed production ancestor.

The ten bodies are fetchOrdersPositionsAndBalance, fetchAndSetPrivateSnapshot, fetchAndSetPrivateSnapshotHelper, tick, syncBalanceUpnlFromPositions, fetchRawBalance, fetchOrders, fetchOrdersHelper, fetchPositions and fetchPositionsHelper. Rust targets are in `src/core_snapshot_readers.rs`. They retain eager independent request ownership, source receiver/getter order, ordered snapshot writes, original allocation/error identities, live retry state, Math.min NaN behavior, UNIFIED fallback, cursor paging, nested async continuation boundaries and deferred mapper lookup. SnapshotHost is venue-local; shared SDK types and traits did not change. Standard value/Promise/property primitives and named maintained dependencies are explicit capabilities. This proves the selected bodies against those contracts, not the whole class/constructor/concrete store/timer/XHR/signer/dependency composition.

| Completed gate | Exit | Measured SHA |
| --- | --- | --- |
| Bybit scoped all-target cargo check, jobs 2 | 0 | `c74fdaa01020cff794f85f055dd25747d54b380a` |
| Focused native baseline, 80 cases, one passed / zero failed / zero ignored | 0 | `c74fdaa01020cff794f85f055dd25747d54b380a` |
| parity:extract:check, output inspected | 0 | `c74fdaa01020cff794f85f055dd25747d54b380a` |
| committed-object tomllib census | 0 | `c74fdaa01020cff794f85f055dd25747d54b380a` |

Commands: `"${CARGO_BIN}" check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2`; `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript "${CARGO_BIN}" test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test core_snapshot_readers --jobs 2 -- --include-ignored`; `node "${YARN_CLI}" parity:extract:check`; `python3.11 sdk-rs/safe-cex-venue-bybit/scripts/core-snapshot-readers-census.py 9fad5d8d1b`. Extractor: 7341 symbols / 512 files; zero drift/additions/orphans. The writer ran after manifest edits, with one updated manifest and zero additions/orphans; that pre-commit writer result is not a release-SHA measurement.

The source oracle extracts these ten original TypeScript members through AST without body edits, uses maintained endpoint/retry constants and installed p-iteration.mapSeries, and stops at case 79 (positions snake-case empty page). Independent family floors: 8 all, 8 ordered snapshot, 4 wrapper, 10 tick, 6 balance sync, 10 raw balance, 6 orders, 10 order paging, 6 positions, 12 position paging. Native decoded variants, UTF-16 units, binary64 values, ordered fields, writes, state, original error/Rc row/info/result identities and loop count run before any supplemental dependency comparisons across all cases. No decode/re-encode wire comparison is used. Native FIFO Promise settlement/drain stops at 256 queued jobs; oracle drain is bounded, not exhaustive scheduling proof.

Read-only sub-agent review at `c74fdaa010` measured source oracle, independent case/family floors, row identity/mapper checks and ten targets/header checks at exit 0; it ran no Cargo, native test, extractor or mutation. Findings corrected before release: NaN-sensitive Math.min, cursor getter ordering before spreads, launcher ownership, genuine nested paging async boundaries and deferred mapper selection. Harness FIFO scheduling and original raw/flattened row pointer checks were corrected before the passing baselines. Two development native compile attempts exited 101 for unused imports and Vec<Value>/Value comparison; corrected development baselines exited 0. A dirty development lib-only check exited 0 before final async correction and is not the scoped release check.

Initial wrapper-control runner finding is retained in `evidence/core-snapshot-readers-controls-initial.json`: expected named iteration 18, but the mutation failed at the earlier named iteration 17, exit 101; restoration passed at 0. This was an invalid runner label, not a survivor. The script label was corrected without changing source/test assertions; remaining controls resumed from index 2 at `143c123d790a63d16aed9c80bc21f918a224c860`. Source/test/oracle/primary-manifest comparison from `c74fdaa010` to that script-only correction exited 0. Completed pairs and final restoration are recorded below; no uncompleted control is included as a gate.

An observed own-worktree macro-loader cache file was refreshed atomically with byte-identical content, exit 0: `sdk-rs/target/debug/deps/libzerovec_derive-0c4a63cb9d4aa14d.dylib`, before/after SHA-256 `2130e47c4b66d4ef9ab346ba3e40830962ae6da183588cd06d31589fb24e24c7`. No source/dependency bytes changed, no other lane cache was changed, and no running gate was terminated.

Not run: full venue tests, workspace compilation, monorepo suites/builds, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live network or full concrete exchange composition. No PR is opened or merged by this agent. Exact gate SHAs remain attached to their measurements; later evidence-only commits do not move them.

Second runner stopped at control index 6 after four valid pairs (indices 2–5). The orders-preservation mutation exited 101 at the earlier native assertion `SNAPSHOT orders iteration 50: native rejection discriminant`, rather than the expected later dependency comparison at iteration 46; restored native baseline exited 0. This second runner-label finding is retained in `evidence/core-snapshot-readers-controls-from2.json`. The expected label was corrected to the actual native failure and controls resume from index 6 without weakening any assertion. Artifact enumeration fields now distinguish attempted from planned controls; no unattempted index is included in a completed floor.

### Completed snapshot-reader controls

Ten distinct successful family pairs were measured across three immutable source SHAs: first two at `c74fdaa01020cff794f85f055dd25747d54b380a`, next four at `143c123d790a63d16aed9c80bc21f918a224c860`, final four at `3931dca437d870f7375dd235422b8c94275eac81`. Each runner baseline exited 0. The final runner exited 0, stopped at control index 9 and verified exact source/test restoration. Successful floor is ten families, not the twelve attempts including the two disclosed label findings. No survivors were observed in these attempted controls. This is a floor, not exhaustive mutation proof.

| Family | Named failure assertion | Mutation / restore | SHA prefix |
| --- | --- | --- | --- |
| parallel raw balance slot | `SNAPSHOT all iteration 0: dependency order and arguments` | 101 / 0 | `c74fdaa010` |
| ordered optional orders | `SNAPSHOT snapshot iteration 8: native store0.orders: native array count` | 101 / 0 | `c74fdaa010` |
| wrapper rejection | `SNAPSHOT wrapper iteration 17: native rejection discriminant` | 101 / 0 | `143c123d79` |
| tick backoff cap | `SNAPSHOT tick iteration 25: native exchange state: native f64` | 101 / 0 | `143c123d79` |
| sync original allocation | `SNAPSHOT sync iteration 30: native store0.balance: native object count` | 101 / 0 | `143c123d79` |
| unified raw balance status | `SNAPSHOT raw iteration 40: native exchange state: native f64` | 101 / 0 | `143c123d79` |
| orders preserve call | `SNAPSHOT orders iteration 50: native rejection discriminant` | 101 / 0 | `3931dca437` |
| order paging limit | `SNAPSHOT ordersHelper iteration 52: eager effects` | 101 / 0 | `3931dca437` |
| empty symbol delegation | `SNAPSHOT positions iteration 64: native array count` | 101 / 0 | `3931dca437` |
| position paging limit | `SNAPSHOT positionsHelper iteration 68: eager effects` | 101 / 0 | `3931dca437` |

Artifacts retain both invalid labels and their actual earlier native failures. Native assertions were not weakened. Scoped source/test/oracle/manifest retention comparison across the runner commits and restored working tree exited 0. Gate exits remain attached to their actual SHA; no later commit is credited with rerunning these mutations.

### Account refresh delivery merge retention at adb9b28f164d5476deef585ebee966aabe7567b9

Merged integration `9fad5d8d1bbb15b743af7af172cbcc5c69238a5a`. Competing append points in lib.rs and this evidence file conflicted; both lanes' exports and evidence blocks were retained in the normal merge. No history rewrite, reset or index deletion was used.

Owned account-refresh input proof is 0 against gate SHA `5450ac69fe0c9aa0568ed5668e40e121fda933c5`. Whole crate source/tests proof is 1 only for lib.rs and the other lane's public_data_recovery source/test. Expanded proof since the prior checked merge is 1 only for the other lane's primary/public manifests and recovery scripts; dependency proof is 0.

Five reruns each exited 0 **at `adb9b28f164d5476deef585ebee966aabe7567b9`**: scoped-cargo-check, account-native, unit-structure, extract-check and variant-notes. The seven original gates and nine completed mutation/restore controls retain their original measured SHA; none is attributed to a later evidence or delivery commit.

Fresh integration-relative census: **9 pending -> ported, 0 -> divergent, 0 regressions**. All Bybit 194/125/201 -> 203/125/192; own ten-file partition 55/63/140 -> 64/63/131 (ported/divergent/pending). Census remains keyed by manifest path plus TS symbol, 26 manifests / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml.

## HyroTrader metadata and execution capability guards

Rows moved: **10 pending -> ported, 0 -> divergent, 0 regressions** against integration `98a847bb171d9a895a95f1e4e287c9126fc2c66e`. All Bybit `203/125/192` -> `213/125/182`; hard ten-file partition `64/63/131` -> `74/63/121` (ported/divergent/pending). Python 3.14 tomllib census keys (manifest path, TS symbol), 26 manifests / 520 rows, stopped at `sdk-rs/parity/manifest/exchanges/bybit/syncUnifiedBalanceUpnlFromPositions.ts.toml`. Only hyrotrader.exchange.ts manifest rows moved: fail, fetchAndCacheApiKeyInfo, fetchApiKeyInfo, getAccount, fetchFees, fetchPositionMode, changePositionMode, changeMarginType, assertPrivateReady and validateOrderCapabilities. No plain-Bybit source/manifest edits by this lane.

Fresh parent `a07e971bb08f867925ff18ca8b13d2f288d0deba`. All seven scoped gate exits and all ten completed controls below were measured at **`49b93c3b6eed01537708256eceffc617d4ed1b00`**, not a later merge/evidence/delivery SHA. No unrelated unreviewed feature branch was used as an ancestor.

Ten native bodies use the unchanged venue-local SessionHost. Original get/optional/get-computed/in/receiver-call/Error/Promise-adoption capabilities retain arbitrary original values, opaque alias identity, UTF-16 text, binary64 flags, live property/getter reads and errors. Async factories evaluate their bodies immediately; metadata return values pass through the retained Promise runtime for adoption. Fetch/store/transport/base-class dependency bodies and a concrete full exchange/SDK host are not newly ported here. Dropped/unpolled Future cancellation and full Promise scheduler composition are not new claims.

Source oracle extracts the ten unchanged original members by AST and the original PositionMode enum. Floor **1683 cases**: 99 declared profiles x 17 fault sites, with independently fixed per-method counts and unique (method, profile, failure) keys in Rust. Stopped at validateOrderCapabilities/getter-changes-market/get:assertPrivateReady#0. Native values/errors, original logged argument identity, returned dependency identity, visible failure count, live socket reads, property-presence short-circuits, four-call continuation for a replaced fail, and fresh mutable fee arrays are asserted before any supplemental comparison. ALL supplemental source receipt comparisons follow ALL native cases. Names/messages/value receipts preserve UTF-16 code units and number bits, including lone surrogates, NaN and signed zero.

| Scoped gate | Exit |
| --- | --- |
| scoped-cargo-check | 0 |
| native-controls | 0 |
| extract-check | 0 |
| unit-structure | 0 |
| partition-notes | 0 |
| scoped-rustfmt | 0 |
| diff-check | 0 |

Exact measured commands:

`cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-capabilities-controls.py`
`node "${YARN_CLI}" parity:extract:check`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-capabilities-structure.py`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-session-notes.py`
`rustfmt --edition 2024 --check sdk-rs/safe-cex-venue-bybit/src/hyro_capabilities.rs sdk-rs/safe-cex-venue-bybit/tests/hyro_capabilities.rs`
`git diff --check`

Native target: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test hyro_capabilities --jobs 2`. Baseline 0 and all ten exact-byte restore tests 0 (one passed / zero failed / zero ignored, 1683 source cases on each passing run). The runner checks a unique edit within each actual new production function, restores exact source bytes in finally, and requires compiled runtime 101 at its named native assertion before counting a control.

| Control | Required native site | Compiled mutation | Exact restore |
| --- | --- | --- | --- |
| fail | `fail_native_error` | 101 | 0 |
| fetch_and_cache_api_key_info | `cache_native_undefined` | 101 | 0 |
| fetch_api_key_info | `api_native_error` | 101 | 0 |
| get_account | `account_native_error` | 101 | 0 |
| fetch_fees | `fees_native_empty` | 101 | 0 |
| fetch_position_mode | `mode_native_oneway` | 101 | 0 |
| change_position_mode | `change_position_native_error` | 101 | 0 |
| change_margin_type | `change_margin_native_error` | 101 | 0 |
| assert_private_ready | `ready_native_error` | 101 | 0 |
| validate_order_capabilities | `capabilities_native_slippage` | 101 | 0 |

No survivors among the ten completed controls attempted; stopped at validate_order_capabilities. Compiler rejects and a 101 from an unrelated site are not included in that claim.

The parity:extract writer ran after metadata preparation: exit 0, updated 0 manifests / 0 added / 0 orphaned. That working-tree writer is not relabeled as a measurement at the subsequent code SHA. Strict unit structure uses original source/export/Cargo bytes, all ten new rows and all **ten new public Rust functions**, zero exemptions. Inherited broad Bybit structure RED is not claimed green and was not rerun. The partition notes gate covers ten assigned manifests / 63 canonical divergent notes. Rustfmt covers the two new Rust files.

SOURCE BEHAVIOR PRESERVED: fail first selects/calls emitAndLogError with the exact original message, then creates and throws a new Error; a throwing logger or constructor/coercion failure wins. Undefined is logged as undefined while new Error(undefined) has an empty message. Metadata failures delegate to the current fail callable and adopt a replacement Promise return/rejection. Empty cache resolves undefined; fees allocate fresh mutable arrays and ignore category; mode always returns the original oneway enum value and ignores symbol. Change-mode guards use JS truthiness (objects/arrays/nonempty strings reject, null/undefined/false/zero/NaN/empty strings pass). Readiness short-circuits started and rereads the current socket before ready, so a getter can replace it between reads. Order capability checks call current assertPrivateReady first, retain store-before-symbol and symbol-before-market ordering, and require quote strictly USDT. Any maxSlippagePct other than undefined, including null/0/NaN or inherited values, rejects. The BBO/TickSize check uses inherited property presence even when the value is undefined, and short-circuits after the first presence. A replaced nonthrowing fail continues through later checks; the multi-failure fixture witnesses all four calls. No source behavior was silently corrected.

PRE-COMMIT INSTRUMENT / REVIEW DISCLOSURES: the first source preflight exited 1 because its expected floor was mistakenly 1700 while the declared product was 95 profiles x 17 sites = 1615. The expectation was corrected by counting the fixed declarations; no cases were removed. The observer was reviewed to retain field-arrow lexical this through tracked getters on the actual allocated instance rather than proxying the owner. The first native preflight compiler exit 101 was an unused harness market field under -D warnings; it was removed, not counted as a runtime control. An uncommitted 1615-case native preflight then passed. Review added Promise adoption for replaced metadata fail returns/rejections, 68 declared cases, raw logged argument pointer assertions and independently fixed per-method counts, producing the 1683-case committed gate. These working-tree findings are not attributed to the base or later committed SHA, and no rejected preflight is included in the seven gates or completed controls above.

Merge-down measurement at `a8af0ad75e6794ce5ae9dc4381467283210e07b6`, fetched integration `98a847bb171d9a895a95f1e4e287c9126fc2c66e`. Retention proofs:

- required: exit 0; changed none.
- owned: exit 0; changed none.
- expanded: exit 0; changed none.
- dependency: exit 1; changed `package.json`, `sdk-rs/Cargo.lock`.

Reruns measured at **`a8af0ad75e6794ce5ae9dc4381467283210e07b6`**, not later delivery:
- scoped-cargo-check: 0.
- capabilities-native: 0.

Not run: full Bybit/workspace/monorepo suites, workspace compilation, host/wasm Clippy, package TS typecheck, broad Bybit structure rerun, live exchange/network/credentials tests, CI rerun, full concrete host/class integration. No shared SDK type/trait was modified; no PR was opened/merged by this lane. Evidence is committed here; raw logs/corpus remain ignored. Final merge/push/remote verification are separate delivery measurements.

Merge dependency detail at `a8af0ad75e6794ce5ae9dc4381467283210e07b6`: Cargo.lock adds safe-cex-num to the existing safe-cex-lighter-sdk dependency list; root package.json adds KuCoin's spot_bodies oracle target. Cargo.toml, Bybit Cargo.toml, shared SDK types/num/exchange/transport/store/crypto, maintained source, all compared Bybit source/tests/scripts/manifests and yarn.lock were unchanged. Scoped all-targets check and hyro_capabilities native target reruns each exited 0 at that merge SHA. The original seven gates and ten completed controls remain measured at `49b93c3b6eed01537708256eceffc617d4ed1b00`.


### Snapshot-reader integration merge and refreshed gates

Integration `9c663a908f40c2955e27b1ce58f97fb5c08fb331` merged down to **`91bd3233069cbcdc8b56e0c93aa68131c12848d6`**. Initial merge exited 1 for competing appended GATE_EVIDENCE sections and lib module registrations; preserving both completed the merge at exit 0. Whole crate source/tests diff from `c74fdaa010` exited 1: integration added HyroTrader capability and variant-account-refresh modules/tests and lib exports. Owned snapshot-reader source/test/oracle/primary-manifest/TS body diff exited 0. Shared runtime/transport and Cargo.lock inputs changed (diff 1), so fresh scoped check and focused native baseline were required. Other lane modules/tests equal pinned integration (diff 0); no direct edits were made to their implementation or manifest rows.

Fresh measurements executed at `91bd3233069cbcdc8b56e0c93aa68131c12848d6`, not at the later evidence/delivery commit: scoped Bybit all-target cargo check exit **0** (Finished dev profile 2m30), focused core_snapshot_readers native test exit **0** (one passed / zero failed / zero ignored; 80 scenarios through index 79), extractor check exit **0** (7341 symbols / 512 files, zero drift/additions/orphans; output text inspected), committed-object tomllib census exit **0**, diff-check exit **0**. Commands are the exact scoped commands above, with census base changed to pinned integration `9c663a908f40c2955e27b1ce58f97fb5c08fb331`.

Refreshed census: 213/125/182 -> 223/125/172 (ported/divergent/pending), exactly **10 pending -> ported, 0 -> divergent**. Independent floor stays 26 manifests / 520 rows, stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml. Other lanes' integration moves are included in both sides rather than credited to this slice. The ten successful mutation pairs remain pinned to their original three SHAs; they were not rerun or relabeled at the merge SHA. All earlier non-claims remain in force.


Final snapshot-reader refresh: merged integration `d1652df92999c41782adbd5be274485279db548b` down at exit 0 to **`37fcdda135704c0c516dc4b36b6a728d75c0119e`**. Required whole Bybit source/tests comparison from scoped-check/native SHA `91bd3233069cbcdc8b56e0c93aa68131c12848d6` exited **0**. Expanded Cargo/config/runtime/source-oracle input comparison exited **0**. Those measured check/native exits stand at their original SHA, without relabeling. Global parity inputs changed (diff 1, Coinbase/Polymarket manifests and exemptions), so fresh extractor check at `37fcdda135` exited **0**, with 7341 symbols / 512 files and zero drift/additions/orphans; output text inspected. Fresh tomllib census at the same SHA against pinned integration `d1652df929` exited **0**: exactly 10 pending→ported / 0→divergent, 213/125/182 → 223/125/172. Later evidence-only commits alter none of these inputs. Final retention paths/exits are tracked in `evidence/core-snapshot-readers-final-retention.json`.


## Primary ledger history

Rows moved: **10 pending -> ported, 0 -> divergent**, independently counted with committed-object Python 3.11 tomllib at feature **`7973af3b7e347057cef26b1240478ca8594be679`** against fresh integration parent **`7d61bf63d82a52cb0da4731131e8542569cc820d`**. Bybit 223/125/172 -> 233/125/162 (ported/divergent/pending). Floor: 26 manifests / 520 rows; stopped at syncUnifiedBalanceUpnlFromPositions.ts.toml. Seven primary exchange rows and three bybit.income.ts rows changed. No inverse/spot/demo/HyroTrader source or manifest row was directly edited, and no unrelated unreviewed feature work is an ancestor.

Selected exchange bodies: fetchClosedPositionRows, fetchHistoricalPositions, fetchClosedPnl, fetchHistoricalOrders, fetchIncomeHistory, fetchIncomeHistoryRange and incomeContext. Income rows: BybitIncomeContext, fetchBybitIncomeHistory and fetchBybitIncomeHistoryRange. Targets are real Rust symbols in src/ledger_history.rs. The generic context retains XHR/log handles and the closed category/account-type unions. No shared SDK type or trait changed. SnapshotHost is the existing venue-local value/Promise capability boundary; property access, iterator protocol/close, Map/Set, coercion, sorting and named maintained helpers are explicit capabilities, not newly claimed concrete SDK implementations.

Raw closed-PnL retains the source20-page cap and rejects failed responses rather than exposing a partial result. Mapped closed-PnL retains pre-catch bounds and per-window stop/warning behavior. Historical orders retain first-id dedupe across ascending windows, final-window cap warning, descending sort and optional limit. The journal leg join retains last-write Map values, live quote reads and normalized order-axis failure. Income retains inherited type-map lookup, source field-read order, ten-page/record caps (including inner NaN stopping with outer-window continuation), decomposition overshoot, inclusive non-overlapping ranges, no dedupe and sleep100 after each completed range chunk. New income wrappers/range/body composition runs in the native host; concrete ledger mappers, query helpers, aggregation, logger, XHR and class/runtime wiring remain explicit dependencies.

| Completed gate | Exit | Measured SHA |
| --- | --- | --- |
| scoped Bybit all-target cargo check, jobs2 | 0 | `7973af3b7e347057cef26b1240478ca8594be679` |
| parity:extract:check, output text inspected | 0 | `7973af3b7e347057cef26b1240478ca8594be679` |
| committed-object tomllib census | 0 | `7973af3b7e347057cef26b1240478ca8594be679` |

Exact commands: `"${CARGO_BIN}" check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2`; `node "${YARN_CLI}" parity:extract:check`; `python3.11 sdk-rs/safe-cex-venue-bybit/scripts/ledger-history-census.py 7d61bf63d8`. Scoped check finished dev profile3m23. Extractor:7341 symbols/512 source files, zero drift/additions/orphans. Preparation first exited1 because expectedSource omitted the full source prefix; it wrote no manifest. A writer then exited0 with zero updates, which is not credited as proof of moved rows. Preparation was corrected, ten rows serialized and headers derived from actual Rust targets; the writer reran at exit0, zero updated/added/orphaned, after those metadata edits. These pre-commit preparation/writer runs are not relabeled as committed gates.

The original seven class members and two income function declarations are loaded by AST and transpiled without body edits; the source interface shape, maintained endpoint/retry-window constants, logging enum and splitTimeRange declaration are independently loaded. The source/native floor is87 cases: raw12, positions10, mapped closed13, historical orders14, context4, income wrapper4, range wrapper4, income16, range10. Stopped at iteration86 (range COMMISSION). Native fields, UTF-16 units, binary64 values, error variants/original identity, captured XHR/log handles, lexical callback owner and original raw/info row Rc identities are asserted before ALL supplemental allocation/dependency/eager-effect comparisons. No wire/roundtrip assertion is used. Native owned Promise settlement/drain is bounded at1024 queued jobs; the oracle also bounds its drain. These are fixture floors, not exhaustive input or scheduling claims. The fixture split-range primitive stops after128 generated windows; the declared cases remain below that bound. No arbitrary custom iterator/proxy/all-host-runtime proof is claimed.

Read-only sub-agent review at the feature SHA measured source oracle, independent87/family/witness checks, ten real targets/header checks and control syntax/unique selectors at exit0. Its first two check attempts exited1 for reviewer accessor spelling and script syntax, then were corrected; it ran no Cargo, native test, extractor or mutation. Review's concrete NaN condition finding was fixed and covered by a two-window truthy bad-limit case. Source oracle setup also corrected lexical-arrow capture and cross-realm error branding before native baselines. The first development native run exited101 before completing its planned84-case floor; the corrected84-case run exited0 (pure-sort callback receiver assertion was incorrectly tied to the root context and was scoped to the actually lexical quote/leg callbacks), then0 at85 and87 cases. A development lib-only check exited0 before the full unit was finished; it is not the all-target release gate. Completed mutation pairs and their committed baselines are recorded below; uncompleted runs are not included as gates.

Not run: full venue tests, workspace compilation, monorepo suites/builds, host Clippy, wasm Clippy, standalone Rust indexer, CI rerun, live network or full concrete exchange/class/dependency composition. No PR is opened or merged by this lane. Later evidence commits do not move the actual gate measurement SHAs.

### Completed ledger-history native controls

At **`7973af3b7e347057cef26b1240478ca8594be679`**, `python3.11 sdk-rs/safe-cex-venue-bybit/scripts/ledger-history-controls.py` exited **0**. Its scoped native command is `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript "${CARGO_BIN}" test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test ledger_history --jobs 2 -- --include-ignored`. Initial baseline and all ten exact restorations exited **0**, one passed / zero failed / zero ignored, reaching all87 declared cases through index86. Mutated runs stop at the named assertions below; they are not credited with reaching the later cases. Ten independent expected families matched ten successful pairs, stopped at control index9. No survivors or invalid controls occurred in this completed run. Exact source/test restoration and the source/test/oracle/owned-manifest comparison to the feature commit exited0.

| Family | Named native failure | Mutation / restoration |
| --- | --- | --- |
| income context type unions | `LEDGER type account union` | 101 / 0 |
| raw closed page cap | `LEDGER raw iteration 8: native array count` | 101 / 0 |
| hedge leg join | `LEDGER positions iteration 12.leg: native undefined` | 101 / 0 |
| mapped closed account | `LEDGER closed iteration 22.account: native UTF16` | 101 / 0 |
| historical first-id dedupe | `LEDGER orders iteration 39: native array count` | 101 / 0 |
| fresh primary income context | `LEDGER context iteration 49.category: native UTF16` | 101 / 0 |
| income wrapper return adoption | `LEDGER wrapperIncome iteration 53: native array count` | 101 / 0 |
| range wrapper return adoption | `LEDGER wrapperRange iteration 57: native array count` | 101 / 0 |
| income NaN cap condition | `LEDGER income iteration 75: native array count` | 101 / 0 |
| inclusive income range advance | `LEDGER wrapperRange iteration 58: native array count` | 101 / 0 |

The type-union control fails before source-case enumeration. Other controls name their actual native iteration. The NaN mutation deliberately restores the reviewed wrong comparison and is caught by the two-window native count witness. Gate exits stay attached to the feature SHA; later evidence or merge commits are not credited with rerunning these controls.

## HyroTrader validated private REST read pipeline

Rows moved: **8 pending -> ported, 0 -> divergent, 0 regressions** against integration `7daf9d4c8c6805aaff3aaee1fbc6a0bbe6eedab2`. All Bybit `223/125/172` -> `231/125/164`; hard ten-file partition `74/63/121` -> `82/63/113` (ported/divergent/pending). Python 3.14 tomllib census keys (manifest path, TS symbol), 26 manifests / 520 rows, stopped at `sdk-rs/parity/manifest/exchanges/bybit/syncUnifiedBalanceUpnlFromPositions.ts.toml`. Only hyrotrader.exchange.ts manifest rows moved: fetchRawBalance, assertPrivateListResponse, mapPosition, fetchOrders, fetchPrivateRows, fetchOrdersHelper, mapValidatedOrders and fetchPositionsHelper. No plain-Bybit source/manifest edits by this lane.

Fresh parent `4afb5dbf48e5f071608ee23c74e367a107695439`. All seven scoped gate exits and all eight completed controls below were measured at **`c5991e1a69b27231aa8ede5f69756fc5fa7c7d2e`**, not a later merge/evidence/delivery SHA. No unrelated unreviewed feature branch was used as an ancestor.

Eight native bodies plus the shared owned lexical-callback dispatcher use the unchanged venue-local AccountRefreshHost/SessionHost. Runtime get/optional/computed-property/typeof/Number/isFinite/array-method/iterator/spread/receiver-call/Error/Promise capabilities retain original values and errors, UTF-16 text, binary64 data, live getters, mutable collections and aliases. Initial request factories execute before the returned Future is polled. Original response-meta, finite-numeric, base/order mappers, dedupe and transport dependency bodies remain separate rows. The source fixture precomputes its response-meta dependency values from declared INPUT before invoking owned bodies; the native driver does not replay owned source receipts or owned return values. Its nested dependency Futures settle synchronously within the bounded adapter. Full concrete SDK/class/host integration, full Promise scheduler composition and dropped/unpolled Future cancellation are not new claims.

Source AST oracle retains the eight original members plus the original fail/isFiniteNumeric dependencies, original response module and ENDPOINTS declaration. Floor **1632 cases**: 102 fixed profiles x 16 fault sites, with independently fixed Rust per-method counts and unique keys. Stopped at fetchPositionsHelper/disposed/dedupe#0. Native values, full UTF-16 error messages, original error and returned row/object aliases, wallet/order/position fields, pagination counts, request arguments/defaults/overrides, fresh position spread and forced numeric index, visible failure effects, and dedupe calls are asserted before ALL supplemental source receipt comparisons. Captured-length flatMap growth/replacement and live symbol iterator growth/replacement are witnessed in dependency fixtures. UTF-16 code units and binary64 bits preserve lone surrogates, NaN and scalar types. The driver stops at 512 polls; paging covers successful page-100 termination and the separate 100-request limit failure.

| Scoped gate | Exit |
| --- | --- |
| scoped-cargo-check | 0 |
| native-controls | 0 |
| extract-check | 0 |
| unit-structure | 0 |
| partition-notes | 0 |
| scoped-rustfmt | 0 |
| diff-check | 0 |

Exact measured commands:

`cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-reads-controls.py`
`node "${YARN_CLI}" parity:extract:check`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-reads-structure.py`
`python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-private-session-notes.py`
`rustfmt --edition 2024 --check sdk-rs/safe-cex-venue-bybit/src/hyro_private_reads.rs sdk-rs/safe-cex-venue-bybit/tests/hyro_private_reads.rs`
`git diff --check`

Native target: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test hyro_private_reads --jobs 2`. Baseline 0 and eight exact-byte restore tests 0 (one passed / zero failed / zero ignored, 1632-case floor on each passing run). The runner verifies a unique edit within each actual new production function, restores exact bytes in finally, and requires compiled runtime 101 at its named native assertion before counting a control.

| Control | Required native site | Compiled mutation | Exact restore |
| --- | --- | --- | --- |
| fetch_raw_balance | `wallet_native_list_identity` | 101 | 0 |
| assert_private_list_response | `list_native_undefined` | 101 | 0 |
| map_position | `position_native_identity` | 101 | 0 |
| fetch_orders | `orders_native_category` | 101 | 0 |
| fetch_private_rows | `pages_native_request_floor` | 101 | 0 |
| fetch_orders_helper | `native_mapped_order_aliases` | 101 | 0 |
| map_validated_orders | `fetchOrders_native_order_count` | 101 | 0 |
| fetch_positions_helper | `positions_helper_native_dedupe_call` | 101 | 0 |

No survivors among the eight completed controls attempted; stopped at fetch_positions_helper. Compiler rejects and a 101 from an unrelated site are not included in that claim. Mapper/helper controls can reach their named native assertion through a newly ported caller in this same pipeline; this is recorded rather than represented as an isolated full host composition test.

The parity:extract writer ran after metadata preparation: exit 0, updated 0 manifests / 0 added / 0 orphaned. This working-tree writer is not relabeled as a measurement at the later code SHA. Strict unit structure uses original source/export/Cargo bytes, eight new rows and all **nine public Rust functions** including the explicitly split/shared callback dispatcher, zero exemptions. Inherited broad Bybit structure is outside the green claim and was not rerun. The notes gate covers ten assigned manifests / 63 canonical divergent notes. Rustfmt covers the two new Rust files.

SOURCE BEHAVIOR PRESERVED: wallet hasBalance accepts finite Number coercions initially, while monetary field validation subsequently rejects boolean/whitespace/null values; only undefined and empty-string fields bypass the finite-numeric predicate. Empty coin names pass the string type check. Field predicates reread each value and select current isFiniteNumeric before the final field getter. Private lists require a USDT suffix and positive order-ID length; size zero bypasses active-position side/price/PnL requirements, while leverage remains positive and present. Undefined/null positionIdx uses zero; mapping validates the original row before a fresh spread forces numeric index 0, then requires strict finite mapped numbers and optional margin. Order mapping validates the source list, flatMaps current mapOrder, checks truthy status/type/side and strict finite nonnegative numbers. Pagination spreads caller params before overriding limit 50/cursor, validates and appends aliased rows before inspecting the next cursor, terminates only on undefined/empty string, rejects null/nonstring/repeated cursors, and performs exactly 100 requests at the limit. Pages and reads ignore disposal because the source bodies do. Position helpers treat empty/absent symbols as one settle-coin query; falsy individual symbols use settleCoin, explicit null defaults remain null, and undefined defaults become linear/USDT. Requests run sequentially over the live symbol iterator, mapping and dedupe happen only after collection, and captured array length governs flatMap callbacks. No source behavior was silently corrected.

PRE-COMMIT INSTRUMENT / REVIEW DISCLOSURES: the first source preflight exited 1 because fault-enabled coin access happened during fixture setup before owned-body invocation. Coin wrapping was moved before fault instrumentation, with the same inputs/cases retained. The first native compiler 101 was an unused copied input helper, removed and not counted as a control. Review separately corrected production numeric-callee selection to precede its row field getter. Native runtime 101 at pages_native_count observed four rows against an expected two: the fixed two profile declares TWO rows per page and TWO pages, so the expectation became four plus alternating original-row aliases; production and fixture stayed unchanged. That assertion repair first produced compiler E0716/101 for a temporary elements() borrow; a retained binding corrected the instrument. Review added native request and base-map argument assertions and scoped pagination-only parameter expectations to paginated reads. A subsequent native runtime 101 at positions_helper_native_dedupe_count exposed the reused empty profile declaring both symbols=[] AND response list=[]; the fixed expectation is zero positions plus one request/dedupe call, with no source/input changes. Once all native cases passed, the source comparison at mapPosition/valid/none found an extra get:then introduced by the observer blanket await of a synchronous member. The observer now directly invokes the three synchronous members and awaits the five async members; production, inputs and assertions were preserved. The resulting uncommitted 1632-case baseline passed before the committed gate/control sequence. No preflight failure is claimed as a control or attributed to the base or later gate SHA.

REJECTED SEQUENCE at `7b0071782a85922f0798985103245a0d3376631f`: scoped all-targets check 0, baseline native 0, first four compiled controls completed 101/0. The fifth fetch_private_rows empty-result mutation compiled/runtime 101 at fetchOrders_native_order_count in a newly ported caller BEFORE its required pages_native_count assertion; runner exited 1. Exact production bytes were restored automatically in finally and verified against the commit, but NO post-control restore test ran for the fifth mutation. That control is not counted completed, and this is NOT a seven-green-gate sequence. The corrective commit adds an independent 100-request assertion before paging outcome assertions for both page-100 and page-limit profiles. Its replacement control coherently truncates paging to 99 by changing both the bound and final-request guard; all existing value/error/count assertions and fixture cases remain. The entire seven-gate/eight-control sequence above was rerun at the correction SHA.

Merge-down measurement at `b38fafe4f464365e43b10c29b25d6cf608bca512`, fetched integration `7daf9d4c8c6805aaff3aaee1fbc6a0bbe6eedab2`. Retention proofs:

- required: exit 1; changed `sdk-rs/safe-cex-venue-bybit/src/core_snapshot_readers.rs`, `sdk-rs/safe-cex-venue-bybit/src/lib.rs`, `sdk-rs/safe-cex-venue-bybit/tests/core_snapshot_readers.rs`.
- owned: exit 0; changed none.
- expanded: exit 1; changed `sdk-rs/parity/manifest/exchanges/bybit/bybit.exchange.ts.toml`, `sdk-rs/safe-cex-venue-bybit/scripts/core-snapshot-readers-census.py`, `sdk-rs/safe-cex-venue-bybit/scripts/core-snapshot-readers-controls.py`, `sdk-rs/safe-cex-venue-bybit/scripts/core-snapshot-readers-fixtures.cjs`.
- dependency: exit 1; changed `package.json`, `sdk-rs/Cargo.lock`, `sdk-rs/safe-cex-exchange/src/websocket.rs`.

Reruns measured at **`b38fafe4f464365e43b10c29b25d6cf608bca512`**, not later delivery:
- scoped-cargo-check: 0.
- private-reads-native: 0.
- unit-structure: 0.
- extract-check: 0.
- variant-notes: 0.

Not run: full Bybit/workspace/monorepo suites, workspace compilation, host/wasm Clippy, package TS typecheck, broad Bybit structure rerun, live exchange/network/credentials tests, CI rerun, full concrete SDK/class/host integration. No shared SDK type/trait was modified; no PR was opened/merged by this lane. Evidence is committed here; raw logs/corpus remain ignored. Final merge/push/remote verification are separate delivery measurements.

Merge dependency detail at `b38fafe4f464365e43b10c29b25d6cf608bca512`: integration adds the other lane's core_snapshot_readers module/test/export and primary manifest/scripts. Cargo.lock adds safe-cex-crypto to Bitrue; root package.json adds KuCoin order_lifecycle. Shared safe-cex-exchange websocket.rs adds an initialized private before_close callback, set_before_close accessor and on_close invocation; it adds no shared trait method or safe-cex-types field. No shared SDK file was edited by this lane. Scoped all-targets check, hyro_private_reads native target, strict unit structure, extractor check and variant notes reruns each exited 0 at this merge SHA. Original seven gates/eight completed controls remain measured at `c5991e1a69b27231aa8ede5f69756fc5fa7c7d2e`.


## 2026-10-14 — HyroTrader snapshot publication and simulator read facade (eight rows)

Moved **8 pending→ported, 0→divergent, 0 regressions**, independently measured with tomllib and `(file, ts)` keys over 26 manifests / 520 rows. Fresh parent `c4caf76e6cdb22b858492ca47796fcb46a212d9e`. At that parent versus the code commit, all Bybit is **231/125/164 → 239/125/156**, and the hard ten-file partition is **82/63/113 → 90/63/105** (ported/divergent/pending). The same eight transitions are remeasured against fetched integration `c54b80192e833f40b00bdebe2de289336cfb6b86` at merge SHA `c3f97cfded885f158209fbe8f823602c188c1dba`. No plain Bybit source or manifest edits by this lane.

New rows: `reconcilePrivateState`, `fetchAndSetPrivateSnapshot`, `fetchOrdersPositionsAndBalance`, `fetchAndSetPrivateSnapshotHelper`, `fetchMarkets`, `getBybitRestBaseUrl`, `getBybitPublicWsUrl`, `getBybitPrivateWsUrl`, all on `HyrotraderExchange`. The strict unit structure gate includes all nine new public Rust functions, including the explicitly split/shared callback dispatcher, and zero exemptions. Existing venue-local Host traits are unchanged. Original TypeScript bodies are unchanged.

Seven gates measured at **`d205be1c6b97eb73e3f0bcc4b632ac7ab2e99174`**, not the later documentation/delivery SHA:

| Gate | Exit |
| --- | --- |
| scoped-cargo-check | 0 |
| native-controls | 0 |
| extract-check | 0 |
| unit-structure | 0 |
| partition-notes | 0 |
| scoped-rustfmt | 0 |
| diff-check | 0 |

Commands: scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2`; `python3.14 sdk-rs/safe-cex-venue-bybit/scripts/hyro-snapshot-controls.py`; direct Yarn CLI `parity:extract:check`; strict eight-row structure script; ten-file partition notes script; `rustfmt --edition 2024 --check` for the two new Rust files; `git diff --check`. The extractor writer ran before the code commit: exit 0, zero updated/added/orphaned symbols; it is not attributed to that later SHA.

The `hyro_snapshot` test target runs a 371-case original-AST oracle: 53 fixed profiles × seven fault selections, with independent unique-key/per-method floors (105 reconcile, 14 per simple wrapper, 42 helper, 28 markets, 56 per URL method) and the declared last iteration. All native cases are asserted before ANY supplemental comparison. Native receipts decode store fields, counters, numeric bits, UTF-16 text, original raw aliases, errors, calls and detached rejections. The fixture Host runs already-settled dependency Promises; it does not constitute a complete Promise scheduler or concrete transport/store/class composition. Source-body outputs/receipts are not replayed as native dependencies.

Native target: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test hyro_snapshot --jobs 2`. Baseline exit 0; each exact-byte restore test exit 0, one passed / zero failed / zero ignored with the full 371-case floor. Eight completed controls at the same code SHA:

| Control | Required native assertion | Compiled mutation | Exact restore |
| --- | --- | --- | --- |
| reconcile_private_state | `snapshot_native_publication_result` | 101 | 0 |
| fetch_and_set_private_snapshot | `snapshot_wrapper_native_call` | 101 | 0 |
| fetch_orders_positions_and_balance | `account_wrapper_native_call` | 101 | 0 |
| fetch_and_set_private_snapshot_helper | `snapshot_helper_native_call` | 101 | 0 |
| fetch_markets | `markets_native_count` | 101 | 0 |
| get_bybit_rest_base_url | `rest_url_native_value` | 101 | 0 |
| get_bybit_public_ws_url | `public_url_native_value` | 101 | 0 |
| get_bybit_private_ws_url | `private_url_native_value` | 101 | 0 |

No survivors among the eight completed controls attempted; stopped at get_bybit_private_ws_url. Compiler rejects and a 101 from an unrelated site are not included in that claim. Each edit is uniquely scoped to its actual public production function and source bytes restore in finally.

SOURCE BEHAVIOR PRESERVED: snapshot version increments before requests and outside the catch; requests start in order and concurrently through Promise.all, while a synchronous later request throw can leave an earlier rejection detached. Disposal/current/version/revision guards return false without publication; wrappers still resolve undefined. Empty orders arrays remain truthy and advance the orders version; false/null includeOrders omit the request and retain existing orders/loaded flags. Store updates precede orders-version increments. The balance keeps extra fields, retains source `+` semantics (string PnL yields `"023"`) before numeric subtraction, and preserves existing loaded keys. Catch logs Error identity or a normalized primitive and then rejects with the original error; emission failure propagates. Reconnecting helper skips reconciliation, but not-started helper proceeds. Market filtering keeps exact USDT and Map replacement retains the last duplicate value at the first insertion position, with original market aliases. Map constructor selection precedes its input evaluation. URL selectors use ordinary computed/inherited property lookup, reject every falsy configuration, retain a truthy raw value including object identity, and the public selector rejects non-linear categories before URL lookup. No source behavior was silently corrected.

PRE-COMMIT INSTRUMENT / REVIEW DISCLOSURES: compiler 101 first required an explicit Result error type for the new async catch, then rejected an unused copied tag helper under -D warnings; both were repaired before native execution and neither is a control. The first compiled native preflight exited 101 at the oracle-success assertion because the source process terminated on an unhandled order rejection when a later request threw before Promise.all attached handlers. The observer now captures and exposes detached rejections per case, with an independently asserted native detached floor/identity; source bodies and all 371 declarations are retained. The next native 101 at snapshot_native_version expected the stale-version hook to have run even when an earlier injected request fault prevented it. The expectation now follows that declared hook reachability; source bodies/inputs and production are unchanged by that repair. A control-recipe audit wrapper separately exited 1 (NameError: missing __file__ in the exec namespace) before baseline or mutation execution; the namespace was corrected to the actual script path and all eight unique recipes audited. Review also moved formatted object spread before its explicit field getters, retained Map constructor selection before filter/map, represented inherited URL configuration with an actual native prototype, and added the unbound current-callback receiver and raw URL identity assertions. Corrected native preflights passed before the committed sequence; none of those failures is claimed as a control or attributed to the base or gate SHA.

Merge-down at **`c3f97cfded885f158209fbe8f823602c188c1dba`**, fetched integration `c54b80192e833f40b00bdebe2de289336cfb6b86`: whole-crate src/tests retention exit 0; owned production/test/scripts/manifest/source retention exit 0; expanded manifest/extractor/variant input retention exit 0. Broader dependency proof exit 1 only for root package.json: integration adds KuCoin's class_construction oracle target. Cargo.lock/Cargo.toml/shared types/traits/numeric/yarn.lock are byte-unchanged. Conservative reruns at THIS merge SHA: scoped-cargo-check=0, snapshot-native=0. Original seven gates/eight controls remain attributed to the earlier code SHA.

Not run: full Bybit/workspace/monorepo suites, workspace compilation, host/wasm Clippy, package TS typecheck, broad Bybit structure rerun, network/credential/live-exchange tests, CI rerun, full concrete SDK/host/class integration, exhaustive scheduler/cancellation proof. No shared SDK type or trait was edited by this lane. No PR is opened or merged by this lane. This authored sheet is tracked/committed; raw logs and corpus stay ignored. Final fetch/merge, input proof, push and remote SHA verification are separate delivery actions.


### Ledger-history first integration merge: compile finding

Integration `faee40c0372da23d6558ad3da511c44a22b05331` merged down to `9e97b68ac1274b9ea7eccd907e3c786b2c5ee42e`. Initial merge exited1 for appended evidence/export conflicts; retaining both completed it at0. Whole Bybit source/tests diff from `7973af3b7e` exited1 (integration's Hyro private reads/snapshot and spot-order modules/tests plus lib exports); owned ledger source/test/oracle/control/two-manifest/TS-body diff exited0, shared dependency diff0. Other-lane implementation comparison to pinned integration exited0.

At `9e97b68ac1274b9ea7eccd907e3c786b2c5ee42e`, the fresh scoped all-target check exited **101**, solely because integration's private `fields` helper at `src/spot_order_writes.rs:92` is unused under denied warnings. It is not a green compile gate. The focused native rerun was not run at that SHA because the compile floor failed. Fresh extractor check exited **0**, 7341 symbols/512 source files, zero drift/additions/orphans, output text inspected. Fresh committed-object tomllib census exited **0** against `faee40c037`:248/125/147 ->258/125/137, exactly10 pending→ported /0→divergent,26 manifests/520 rows through syncUnifiedBalanceUpnlFromPositions.ts.toml. These measurements are not relabeled after a repair. A fetch of integration after the failure exited0 and returned the same head.


Narrow integration compile repair: removed ONLY the private unused `fields` helper from inherited `src/spot_order_writes.rs`. Read-only sub-agent audit at `9e97b68ac1274b9ea7eccd907e3c786b2c5ee42e` measured static declaration/reference/manifest/exemption/inheritance checks at0: its declaration was the sole identifier occurrence, zero parity targets named it, and the pre-repair module matched pinned integration. An initial reviewer path search exited2, then corrected searches succeeded. The reviewer ran no Cargo and made no edits. This is the only direct implementation edit outside the owned ten-row ledger unit, required by the observed integration compile-floor failure; no spot TS body or manifest row was changed, and it is not credited as porting an extra row. Denied warnings and runtime behavior remain intact; restored scoped gates must pass separately.


### Ledger-history repaired integration gates

At **`33e02350da9a79218985860f5f41660accb2dae2`**, fresh scoped Bybit all-target check exited **0** (Finished dev profile3m43), fresh focused ledger_history native test exited **0** (one passed / zero failed / zero ignored; all87 cases through index86), and fresh committed-object tomllib census exited **0** against pinned `faee40c037`: exactly10 pending→ported /0→divergent,248/125/147 →258/125/137. Commands are the scoped commands above; native command carries the required SAFE_CEX_TYPESCRIPT_PATH and jobs2.

Owned ledger source/test/oracle/control inputs compare to feature `7973af3b7e` at **0**; its ten mutation pairs retain that original SHA and were not rerun at the repair SHA. Global extractor TS/manifest/package inputs compare to the actual extractor SHA `9e97b68ac1` at **0**, retaining that measured extractor0 without relabeling. Independent comparison of the spot module against pinned integration, with only the observed unused helper removed, exited **0**. The former failing merged check101 remains disclosed above. No warning exemption or parity target was introduced. Following evidence commits change none of these compiled/native/extractor inputs; final merge-down preservation is recorded separately.


Final ledger-history refresh: integration `2a0e1813663e4b5bb627bfde9db765cd958c5d84` merged down at exit0 to **`7abbe8b2b56babd1e3eb2fe2e4f8735971f603bf`**. Required full Bybit source/tests comparison from the repaired check/native SHA `33e02350da9a79218985860f5f41660accb2dae2` exited **0**; expanded scripts/Cargo/config/shared-runtime/source-oracle input comparison exited **0**. Those measured check/native exits remain pinned to `33e02350da`, without relabeling. Global parity inputs changed (diff1: Crypto.com/OKX/Lighter manifests and exemptions), so a fresh extractor check at `7abbe8b2b5` exited **0**,7341 symbols/512 source files, zero drift/additions/orphans; output text inspected. Fresh committed-object tomllib census at that same SHA against pinned `2a0e181366` exited **0**: exactly10 pending→ported /0→divergent,248/125/147→258/125/137. The10 mutation pairs remain attached to `7973af3b7e`. Full retention paths/exits are tracked in evidence/ledger-history-final-retention.json; subsequent evidence-only commits do not change gate inputs. The scoped compile failure at the first merged SHA and its single unused-helper repair remain disclosed above. Full venue/workspace/Clippy/network/composition non-claims still apply.


## 2026-10-14 — HyroTrader order observation, reconciliation and publication (eight rows)

Moved **8 pending→ported, 0→divergent, 0 regressions**, independently measured with tomllib and `(file, ts)` keys over 26 manifests / 520 rows. Fresh parent `1f7e8851f837a12a309df28d0e3aedade840b44c`. The user changed Bybit to a three-way partition before this slice: this lane now audits only inverse exchange/private/public sockets, HyroTrader exchange/order-recovery, and demo. There was no uncommitted spot work at handoff, and no spot or plain-Bybit source/manifest edit by this lane. Our revised six-file partition is **58/42/72 → 66/42/64** (ported/divergent/pending). Against the original parent all Bybit is **239/125/156 → 247/125/148**; against fetched integration `372ef194e884c18cb92533b00a097952a00304b5` at merge `6697eb2f59b7330c59f06fdd696fe887c70ecf6d` it is **258/125/137 → 266/125/129**, retaining other lanes' rows. The recovery constructor/four supplied callback fields and full class composition remain pending.

New rows: `HyroOrderRecovery.observeOrders`, `.dispose`, `#unknown`, `#runGroup`, `#findOrder`, and `HyrotraderExchange.observePrivateOrders`, `.submitCreateOrders`, `.cancelOrdersHelper`. Strict structure covers these eight rows in two original manifests and all nine new public Rust functions, including the explicitly split/shared lexical callback dispatcher, with zero exemptions. Existing venue-local Host traits and original TypeScript bodies are unchanged.

Seven gates measured at **`719c1a96e81917b10d2cb842793d06ec6631db2f`**, not later documentation/delivery:

| Gate | Exit |
| --- | --- |
| scoped-cargo-check | 0 |
| native-controls | 0 |
| extract-check | 0 |
| unit-structure | 0 |
| partition-notes | 0 |
| scoped-rustfmt | 0 |
| diff-check | 0 |

Commands: scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2`; order-publication controls script; direct Yarn CLI `parity:extract:check`; strict eight-row structure script; revised six-file partition notes script; `rustfmt --edition 2024 --check` for the two new Rust files; `git diff --check`. The extractor writer ran before the code commit: exit 0, zero updated/added/orphaned symbols; it is not relabeled as a measurement at the code SHA.

The `hyro_order_publication` native target runs a 441-case original-AST oracle: 63 fixed profiles × seven fault selections. Independent unique-key/per-method floors are 42 observeOrders, 14 dispose, 49 unknown, 56 runGroup, 112 findOrder, 35 observePrivateOrders, 70 submitCreateOrders and 63 cancelOrdersHelper, plus the declared last iteration. ALL native cases are asserted before ANY supplemental comparison. Native assertions decode observed-ID sets, disposal state, original response/create/order aliases, uncertain/authoritative/recorded flags, publication/retirement effects, error identities, request arguments and paging bounds. Receipts retain UTF-16 text and numeric bits, and also compare call count before the first await. Async entry is eagerly driven to first suspension; the fixture wait capability defers every await once and the completion driver is bounded to 1024 polls. This adapter is not a complete scheduler, concrete transport/store/class assembly or a dropped-Future cancellation proof. Source outputs/receipts are never replayed as native dependencies.

Native target: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test hyro_order_publication --jobs 2`. Baseline exit 0; each exact-byte restore test exit 0, one passed / zero failed / zero ignored with the full 441-case floor. Eight completed controls at the same code SHA:

| Control | Required native assertion | Compiled mutation | Exact restore |
| --- | --- | --- | --- |
| observe_orders | `observe_native_linkset` | 101 | 0 |
| dispose | `dispose_native_pending_cleared` | 101 | 0 |
| unknown | `unknown_native_message` | 101 | 0 |
| run_group | `group_native_authoritative_seen` | 101 | 0 |
| find_order | `find_native_request_floor` | 101 | 0 |
| observe_private_orders | `private_observe_native_ids` | 101 | 0 |
| submit_create_orders | `publication_native_recorded` | 101 | 0 |
| cancel_orders_helper | `cancel_native_original_aliases` | 101 | 0 |

No survivors among the eight completed controls attempted; stopped at cancel_orders_helper. Compiler rejects and a 101 from an unrelated site are not included in that claim. Each edit is unique within its actual public production function and source bytes restore in finally. The paging control coherently changes BOTH the loop bound from 100 to 99 and the terminal guard from page 99 to 98, then fails the independently declared native 100-request floor before outcome comparisons. No failed/rejected committed sequence occurred in this slice.

SOURCE BEHAVIOR PRESERVED: observation skips non-string link/order IDs but accepts empty strings; Set registration deduplicates repeated IDs. Recovery disposal clears the current pending map. Unknown-outcome Error construction retains source message coercion and legal lone UTF-16 surrogates. Group publication resumes at existing response length, rereads the live creates length (including acceptance callback growth), makes a stale/disposed create uncertain, sets authoritativeSeen before pushing the original response, and invokes onAccepted AFTER that push. Thus an onAccepted throw retains the already-appended response. Reconciliation checks open orders before history, returns the original matched row including Rejected status, requires a nonempty string identity and a declared status, stops only on undefined/empty cursor, rejects null/nonstring/repeated cursors, and rejects after exactly 100 requests at a continuing cursor; a found order on request 100 succeeds. Publication marks recorded before disposal/observation/authoritative guards, removes terminal parent/bracket IDs, maps recovered orders, records only fresh unseen unreconciled responses, emits/rethrows original errors, and removes only its captured observations Set in finally. Cancellation is sequential, accepts the original case-insensitive gone/too-late messages despite a failed response status, retires the original order plus the current store's original bracket-child aliases, and retains a completed first retirement when the second POST rejects. No source behavior was silently corrected.

PRE-COMMIT REVIEW / INSTRUMENT RECORD: original source preflight and each native preflight actually run exited 0; the recipe audit parsed the runner AST and verified eight unique edits before any mutation. Review retained the unbound acceptance-callback receiver, original raw group/response/order aliases, endpoint evaluation before cancellation payload field reads, RegExp literal/test selection before retMsg access, normal/abrupt iterator closing, and raw loop index selection before relational coercion. These are review corrections before committed gates, not failed controls. There are no invented compiler rejects, instrument failures or rejected sequences to report for this unit. Earlier slice failures documented above are not attributed to this one.

Merge-down at **`6697eb2f59b7330c59f06fdd696fe887c70ecf6d`**, fetched integration `372ef194e884c18cb92533b00a097952a00304b5`. Retention proofs:

- required: exit 1; changed `sdk-rs/safe-cex-venue-bybit/src/ledger_history.rs`, `sdk-rs/safe-cex-venue-bybit/src/lib.rs`, `sdk-rs/safe-cex-venue-bybit/src/spot_order_writes.rs`, `sdk-rs/safe-cex-venue-bybit/tests/ledger_history.rs`, `sdk-rs/safe-cex-venue-bybit/tests/spot-order-writes-oracle.mjs`, `sdk-rs/safe-cex-venue-bybit/tests/spot_order_writes.rs`.
- owned: exit 0; changed none.
- expanded: exit 1; changed `sdk-rs/parity/manifest/exchanges/bybit/bybit.exchange.ts.toml`, `sdk-rs/parity/manifest/exchanges/bybit/bybit.income.ts.toml`, `sdk-rs/parity/manifest/exchanges/bybit/bybitspot.exchange.ts.toml`, `sdk-rs/safe-cex-venue-bybit/scripts/ledger-history-census.py`, `sdk-rs/safe-cex-venue-bybit/scripts/ledger-history-controls.py`, `sdk-rs/safe-cex-venue-bybit/scripts/ledger-history-fixtures.cjs`.
- dependency: exit 0; changed none.

The whole crate/expanded diffs come from the other lanes' ledger_history and spot_order_writes modules/tests/exports and their core/income/spot manifests/scripts. Both lanes' exports were retained by the normal merge; no reset, history rewrite or force push. Owned inputs are byte-unchanged. Cargo.lock/Cargo.toml/shared types/traits/numeric/root package.json/yarn.lock are unchanged. Reruns measured at this merge SHA: scoped-cargo-check=0, order-publication-native=0, unit-structure=0, extract-check=0, variant-notes=0. Original seven gates/eight controls remain measured at the earlier code SHA.

Not run: full Bybit/workspace/monorepo suites, workspace compilation, host/wasm Clippy, package TS typecheck, broad Bybit structure rerun, network/credential/live-exchange tests, CI rerun, complete scheduler/cancellation proof, full concrete SDK/host/class integration. No shared SDK type/trait was edited by this lane. No PR is opened or merged by this lane. CLAUDE.md and agent-memory were not reread. This authored sheet is tracked/committed; raw logs and corpus stay ignored. Final fetch/merge, retention proofs, push and remote SHA verification are separate delivery actions.

## Resumed inverse reads: eight rows accepted after committed proof

**8 pending rows moved, all ported, zero regressions.** Preserved checkpoint
44806fb80308f3441f1d43d69286f415c661c805 was merged normally into a fresh branch
from integration 0445a753756f197e4363222eab9e787868e4f700. The owned timer
cancellation and Coinbase/CI-wiring integration changes are included. No
checkpoint history was discarded, rewritten or stashed.

The eight inverse rows are cachedApiKeyInfo, fetchRawBalance, formatBalance,
fetchOrdersHelper, fetchMarkets, fetchIncomeHistory, fetchIncomeHistoryRange and
incomeContext. The committed tomllib census at c17d433c0ee9311156af4a9d6b19c8e3ce289786
retains the same 26 manifests and 520 unique file-plus-symbol keys:
266 ported / 125 divergent / 129 pending becomes 274 / 125 / 121. Exactly those
eight rows move pending to ported; no other status changes occur. The three
existing Bybit FINDING rows remain open.

### Complete sequence at c17d433c0ee9311156af4a9d6b19c8e3ce289786

| Named gate | Exit |
| --- | --- |
| scoped-cargo-check | 0 |
| native-controls | 0 |
| extract-check | 0 |
| unit-structure | 0 |
| unit-notes | 0 |
| scoped-rustfmt | 0 |
| diff-check | 0 |

Cargo calls were serial, locked and jobs 1. The baseline target passed six tests:
the 488-case corpus and five separately named boundary tests. The corpus asserts
all native cases before its supplemental comparisons and independently checks
the exact 61-profile by eight-fault inventory. Independent source measurement
confirms all 427 non-none fault configurations: 111 reached, 316 unreached, zero
mismatches. Reached faults assert original error identity; unreached faults
execute normal native assertions. Kind, account parameters and margin status
also have independent native observations.

Each control compiled, exited 101 at its required native assertion, restored
exact production bytes and passed the same exact test at exit 0. Failed targets
each report one failed test; restores each report one passed test. No survivors
among thirteen controls attempted. Compiler rejects, wrong-site failures and
incomplete restoration sequences are excluded.

| Control | Required native assertion |
| --- | --- |
| wallet-force | wallet_native_force |
| balance-total | balance_native_total |
| orders-limit | orders_native_limit |
| market-alias | markets_native_original_alias |
| income-arguments | income_native_arguments |
| income-range-arguments | income_range_native_arguments |
| income-context | income_context_native_through_delegate |
| cached-identity | cache_native_identity |
| balance-strict-length | balance_native_strict_length |
| orders-strict-length | orders_native_strict_length |
| market-iterable-interleave | markets_native_then_before_next |
| market-explicit-receiver | markets_native_explicit_all_receiver |
| market-all-capture | markets_native_boundary_outcome markets-all-capture-before-map |

### Source-first corrections and rejected attempt

Static review found the checkpoint's undefined h in a pre-invocation request
loop before any compiler run at that checkpoint. This is not an observed
compiler rejection. Assertions now follow an independent native request count.

At d34cdcbbab, unchanged production compiled and the target exited 101: the
original 488-case oracle passed, while all five new boundaries failed at their
intended native assertions. The repair retains strict zero-length equality,
selects Promise.all with its receiver before argument evaluation and passes the
original iterable without precollection. The original map error still takes
precedence over a selected noncallable all value.

Review repaired three observer gaps before acceptance: an extra native all-getter
observation absent from the interleave source fixture, missing independent
no-warning assertions, and receiver_call ignoring its receiver. Source bodies
and cases were not changed to fit the observer. Exact committed assembly checks
confirmed real newlines, zero parse diagnostics and exact member inventories;
the suspected escaping fault was not reproduced and no repair was invented.
The Promise witness is a profile-scoped host capability simulation, not a
complete native JavaScript Promise runtime.

The 0208bc6095 sequence was WITHDRAWN: scoped check 0 and native baseline 0;
wallet mutation output reached wallet_native_force, but deliberate interruption
measured exit -2, runner exit 1 and automatic production restoration. No
post-control restore test ran: ZERO completed controls and NOT seven green gates.
Tool probes returned 126 because Volta could not find Node; direct installed
Node/Yarn reported 1.22.21 at exit 0. The runner and manifest-file inventory guard
were fixed, then all seven gates and thirteen controls reran at c17d433c0e.
The interruption is not attributed to disk pressure.

Annotation acceptance follows proof at 9cf8695ed1: six operational bodies are
adapted, two pure accessors mirror; the recursive order helper's loop expansion
is explicit. Extract-check, unit-structure and unit-notes each reran there at 0.
The seven-gate/thirteen-control proof belongs only to c17d433c0e, not annotation
or delivery. No runtime input changed during annotation acceptance.

The sub-agent's narrow MERGE review checked the exact census, seven root receipts
and all twenty-six failure/restore logs. It independently ran source programs
and fault-reach measurements, but no Cargo. Full concrete SDK/class composition,
complete scheduler behavior and dropped-Future cancellation remain unclaimed.
The user owns PR merging; this lane delivers a pushed, ready PR.

Not run: full Bybit/workspace/monorepo suites, workspace compilation, host/wasm
Clippy, package TS typecheck, broad Bybit structure, live/network/credential
requests, CI reruns or historical native controls outside this unit. No shared
SDK type or trait changed. Authored evidence is committed; raw logs, generated
reports and corpora remain ignored.


## Inverse-read no-op waker lint repair

Rows moved: 0. No manifest annotation changes. A Python tomllib census retains
26 Bybit exchange manifests, 520 unique file-plus-symbol keys and the triple
274 ported / 125 divergent / 121 pending. The factory remains pending.

At unchanged baseline d19d2874e0265c85602b9240d618612c65984c02,
workspace all-targets keep-going check exited 0, host workspace Clippy exited
101, and the canonical wasm runner exited 1 with underlying Cargo 101. Both
Clippy failures identify manual_noop_waker in inverse_read_surface. The wasm
runner stopped there; no conclusion about later crates follows from that run.

The repair replaces the test's custom inert Wake implementation with
Waker::noop. It preserves the 1024-poll bound and every runtime assertion.
The following serial measurements belong to committed revision
12b3a590b6eb85ea12bed898e8bdf15dbae5d273, not a later evidence or delivery
revision. The tracked tree stayed unchanged throughout the sequence.

| Named gate | Command | Exit |
| --- | --- | --- |
| host-workspace-clippy | cargo clippy --manifest-path sdk-rs/Cargo.toml --locked --workspace --all-targets --keep-going --jobs 1 -- -D warnings | 0 |
| wasm-workspace-clippy | node .github/scripts/rust-wasm.mjs --clippy | 0 |
| inverse-read-runtime | cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-bybit --test inverse_read_surface --jobs 1 -- --include-ignored | 0 |
| host-workspace-build | cargo build --manifest-path sdk-rs/Cargo.toml --locked --workspace --jobs 1 | 0 |

The runtime target executed six tests: six passed, zero failed, zero ignored.
Root Cargo calls use jobs 1; the canonical wasm runner retains its own jobs 2
policy. Raw logs and machine-generated reports remain ignored.

The sub-agent's narrow static MERGE review confirms equivalent inert waking,
unchanged bounds/assertions and no production or manifest change. It makes no
independent runtime claim. This repair does not rerun or renew the earlier
thirteen mutation controls, whose acceptance remains attributed to
c17d433c0ee9311156af4a9d6b19c8e3ce289786.

Not run for this repair: full local test suites, wasm build, package TypeScript
typecheck, broad Bybit structure, live or credential requests, historical
mutation controls, or CI reruns. Host workspace build and both enforced Clippy
lanes were run. The user owns PR merging; readiness does not claim a green
integration CI tip.


Delivery merged integration d74a1c01f7 at
3c5e14a1ef8a7c7642bb7a81bcc13a6926998189. Incoming changes concern CI
dependency-loader JavaScript, its tests and audit documents; no Rust source,
Cargo manifest/lockfile, canonical wasm runner, maintained safe-cex source,
root package manifest or yarn.lock changed from the measured repair tree.
Because dependency resolution changed, inverse-read-runtime reran at that merge
revision: exit 0, six passed, zero failed, zero ignored. The earlier four-gate
sequence remains attributed to 12b3a590b6, not the merge or later delivery.
Scoped rustfmt and diff-check also exited 0 before this evidence acceptance.

## Factory checkpoint and checker provenance

Rows moved: **0**. The independent Bybit TOML census remains 274 ported /
125 divergent / 121 pending. `createAPI` remains pending with no Rust mapping;
neither source preflights nor this diagnostic investigation accept that row.

| Measured revision | Gate | Exit and scope |
| --- | --- | --- |
| `032ab561e7a8456d00996ade783d072e42fd89a2` | workspace all-targets check, locked, keep-going, jobs 1 | 0; unchanged tracked inputs |
| same | source extractor check | 0 |
| same | locked build of `parity-check`, jobs 1 | 0; Cargo-validated artifact, not a clean-build claim |
| same | full workspace structure | 1; stops on webworker `thread_local` before annotation checks |
| same | complete-source receipt dependency scope | 1; stops on imported `CrossTradeLoop` owner before annotation checks |
| `163727c7a730f9eb4e5b11f4869e7fba9d574f67` | locked build of repaired `parity-check`, jobs 1 | 0; includes the already merged PR 7854 repair |
| same | focused `impl_owners` gate fixtures | 0; four passed, 68 filtered; no whole-suite claim |
| same | full current workspace structure | 1; indexing completes and emits 5,904 diagnostics |

The old scoped rejection is a source-indexing false positive: a same-file
`CrossTradeLoop` struct beside `pub use types::*` triggers the conservative
import rule. An independent archive of integration
`333fd419fdbf70f081039610f22da9ee653bc96f` reproduced that scoped rejection with
the same earlier binary. The subsequently Cargo-built old checker reproduced
it too. No exemption, source trimming, or manifest bypass was introduced.
PR 7854 already repairs the indexing rule; this branch merges that repair
rather than implementing a competing one.

The repaired checker binary has SHA256
`eaa00a46e6d7a736a3b635ad6f517852ac85ce7977058d901fb4e5cb16b373f1`.
Against the exact archived workspace, source, manifests and exemptions at
`fc4c3a501ae53384bdc3fa0e847527a87a763c16`, it exits **1** and reproduces the
canonical 5,895-line corpus byte-for-byte, including order. That reference
was measured at `ba57ffe69d813b4fb1de6ab67b9784618c162efa`; its SHA256 is
`e5ec082ef3a64c6cbb76516a6b886db8eadb503ab34af931554f3794d5fcade3`.
Complete multiset additions and removals are both zero. Red output matching
the reference is not a green parity gate.

The initial frozen-reference attempt was **REJECTED**: the archive included
`sdk-rs` but omitted its externally referenced
`infra/tradetunnel/src/frames.rs`, so the checker stopped on a missing file.
That one-line failure is neither canonical output nor a successful comparison.
Its log and receipt were preserved before adding the exact regular file from
the same frozen SHA. The corrected run fingerprints the entire archive,
including that external file, and verifies all inputs and the binary unchanged.

The old glob-owner abort accounts for **0 of the 5,895 diagnostics**. The
canonical corpus already used the repaired index. Its classes remain 3,754
unmapped public items, 1,247 rename declarations, 662 source declarations,
126 unresolved targets, 68 layers, 36 unused declarations and two stale
exemptions. The current 5,904-line output adds exactly nine unmapped behavior
items introduced by the unannotated factory/receipt implementation, removing
none of the reference diagnostics. These are two HttpClient receipt executors,
the Axios response transform, RetryPolicy and WriteRequest receipt executors,
the network predicate, receipt signature retry, the pure Bybit retry predicate,
and `create_api`; plumbing exemptions do not cover these behaviors.

CrossTrade has zero source-row diagnostics in both captures, and the same
13 unclaimed Rust declarations: ten trait members and `failure_entry`,
`CrossTradeLoop::is_disposed`, and `CrossTradeLoopFn`. Its separate 26-row
audit at `702fc59dd864719b85af90c2dde5c7bd5003a241` labels every verdict a
justification finding, not a newly measured runtime defect. The indexing
repair invalidates **0 of those 26 findings**. Eleven rows mapping inherent
methods are exposure to an index-wide abort, not eleven false findings.
The canonical report's no-reproduced-false-positive statement is explicitly
limited to its sampled annotation cases. This reconciliation does not
recertify the behavior of the 26 rows or recommend changing gate severity.

At `4efe2bbbaa`, the factory target exits 0 with 13 tests, 84 request profiles
and four separately counted optional-config helper observations. The predicted
uppercase-GET defect was disproved: RequestClient already lowercases every
dispatch, and the new native method/HMAC assertions pass without a production
repair. This is a positive witness, not a mutation control.
At `042f0519f5bd3b2ec9068015b8abeb9b3a86adbc`, two further source-valid draft
discrepancies compile and exit 101 at their required native sites:
`factory_retry_config_native_empty_method_reentry_methods` and
`factory_region_native_construction 3`. They witness an empty method on a
signature retry and an empty tunnel region respectively. These are unrepaired
draft failures, not completed mutation controls or accepted factory parity.

Independent sub-agent review reconciled the complete captures, source
revision differences, CrossTrade identities and rejected archive sequence.
It ran no Cargo and made no behavioral certification. At this checkpoint,
factory mutation controls, host/wasm workspace Clippy and workspace build have
not run on the factory changes. Historical repair exits retain their earlier
SHAs; neither this evidence commit nor its delivery inherits those measurements.

## Factory repairs and cancellation revalidation checkpoint

Rows moved: **0**. TOML remains 274 ported / 125 divergent / 121 pending
over 520 unique source-file/symbol keys. `createAPI` remains unannotated.

At `5798c2fe6e9eb68ec7cef576a9c19c4f088b4a9e`, the repaired factory target
exits 0 with fourteen passing tests. At
`8ab9a515c3569d2c7c4e013bc5a9699c4ef184fa`, its expanded target exits 0
with fifteen passing tests, no failures or ignored tests. It observes 85
request profiles, five construction profiles, four optional-config helper
cases and four separate astral-param refusals before supplemental comparisons.
These are ordinary runtime witnesses; no mutation control has run yet.

The empty-method and arbitrary-region repairs follow the required native
failures at `042f0519f5bd3b2ec9068015b8abeb9b3a86adbc`. Every dispatch now
defaults an empty method to get before signing. Rejection predicates still
observe the returned config first. Region selection retains JA/SG and forwards
other strings, including empty, to the provider instead of refusing them.
Sub-agent static review found no remaining consumer relying on region Copy.

The first post-method attempt at
`098e6ab150e7af2b076241a3930515c8efe54e3d` failed compilation, exit 101,
on a missing incremental object. No native assertion or control completed.
The cause was initially unclaimed; the user subsequently confirmed that a
concurrent purge of live incremental caches caused it. Later runtime passes
explicitly use CARGO_INCREMENTAL=0. That confirmation does not turn the failed
compilation into runtime evidence.

The committed sequence at
`a2fe9f5d4dc205da7947190ab145b86f86d2539c` is **WITHDRAWN**. Its first
cancellation-classification target compiled, then exited 101 on a stale
guard dependency fingerprint; the runner exited 1. No full gate sequence or
mutation controls completed. Separately, static review found that the selected
regional test did not execute the newly added raw-region assertions. The
corrected runner names that exact test and includes the applicable host/wasm
feature builds. Its declaration is 24 gates; this checkpoint does not claim
that sequence passed. Only the runner script changed while the rejected first
Cargo command was running; its compiled inputs were unchanged.

Cancellation review traced six stale fingerprints to incoming commits
`aefc38cb467b6bf4193ae50977ab3485cb50aa30` and
`f5473d24e35a22fef8d32f029b2cf56012ce603b`, introduced through merge
`032ab561e7a8456d00996ade783d072e42fd89a2`. Proxy broadcast numeric adapters,
WebSocket diagnostic comments and stats-cleanup assertions changed coarse file
hashes. Independent review found the relevant lifetime implementations and
eleven named witnesses unchanged. Only the six reviewed dependency pins and
their explanations were reconciled.

Fresh compiled discovery at
`9711c7db0b82d269368de0d689997e5dc11cfc19` exits 0 and discovers 151
candidates. All previous 150 producer hashes and boundary arrays are unchanged,
with zero removals. The additional opaque-macro candidate is the synchronous
BroadcastMessage numeric-output check. The first validator and classification
preflights reject that unclassified candidate (1 and 101); they are not accepted
gates. Review traced its exhaustive match to concrete String/f64 checks, with
no callback, future or owned lifecycle state, before classifying it immediate.
At `0d6c1a2727aaf0e6a91dcfaa7a85b035ea284b91`, the official Node validator
and compiled classification target both exit 0. These are 151 discovery
candidates, not 151 actual asynchronous creators or a semantic safety proof.

At `8ab9a515c3569d2c7c4e013bc5a9699c4ef184fa`, all eleven existing guard
witnesses pass individually under all features, each with its exact qualified
test name and one passing test. An earlier helper selected a source filename
as a Rust module: Cargo exited 0 with zero tests, and the named-test floor
rejected it. That receipt is preserved. Actual Cargo test enumeration established
the qualified names; the complete corrected eleven-test sequence then passed.
These task-drop witnesses are separate from the thirteen planned mutations.

The astral-param discrepancy is now paired behavior evidence. Maintained source
accepts scalar params containing a well-formed pair, materializes D83D and DE00
as separate property values, and dispatches the actual qs query
`0=%F0%9F%90%80&1=%F2%90%80%80&requestId=id-1`. Rust refuses the complete
request for both native and legacy carriers in signed and unsigned modes,
retains the original scalar input and reports the common failure hook before
request hooks. The source observer records materialized units before JSON;
its output never asks serde_json to retain a lone surrogate. This is a named
input FINDING, not FORCED or ported parity. No signer-call count is inferred
from the missing signature header. The exact witnesses complement
sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary and
sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict;
they do not redeclare or widen either rule.

Factory host/wasm lint and build acceptance, the corrected full gate sequence,
and the thirteen compiled mutation controls remain outstanding at this
checkpoint. The row will not move on these preflights alone.

## Factory sequence rejected on stale lint expectations

The corrected 24-gate run at
`cdbe76d5d8996fd7d384b9f0c6bc69f6cdd0baa1` is **REJECTED**. Its first
thirteen native gates and workspace all-targets check exited 0. Host workspace
Clippy then exited 101 on exactly two unfulfilled arc_with_non_send_sync
expectations in the new HTTP receipt tests; the runner exited 1. The remaining
nine gates did not run. This is not a completed acceptance sequence, and zero
compiled mutation controls were attempted or completed. The entire rejected
receipt and logs were preserved before restarting.

Both tests construct Send/Sync-compatible adapter values and no longer trigger
that lint. The repair removes only their stale expect attributes: no production
code, native assertion, lint policy or oracle input is weakened. All 24 gates
and all thirteen planned controls remain required at the next committed tree.
Rows moved remain 0; the factory row remains pending.

## Factory native acceptance and manifest reconciliation

Rows moved: **1**, from pending to divergent/FINDING; **0 newly ported**.
The file-plus-symbol TOML census changes from 274 ported / 125 divergent /
121 pending to **274 ported / 126 divergent / 120 pending**, over the same
520 unique rows. The open input finding is retained rather than called parity.

The complete committed sequence at
`4b241b5120691effb5449e9afe4e456d5adf5329` exits **0**, with all 24 gates
accepted and their native test floors observed. No tracked source or HEAD
changed during that sequence. These measurements belong to that proof revision,
not this later annotation/evidence commit or delivery.

| Named gate | Exit |
| --- | --- |
| cancellation-classification | 0 |
| shared-result-lifetimes | 0 |
| http-receipt-codec | 0 |
| http-receipt-transform-hooks | 0 |
| transport-response-transform | 0 |
| transport-receipts | 0 |
| transport-effective-config | 0 |
| transport-signature-retry | 0 |
| blofin-effective-config | 0 |
| hyperliquid-effective-config | 0 |
| factory-source-and-native | 0; fifteen passed, zero failed/ignored |
| regional-provider-forwarding | 0 |
| regional-provider-raw-values | 0 |
| workspace-all-targets-check | 0 |
| host-workspace-clippy | 0 |
| host-exchange-all-features-clippy | 0 |
| wasm-workspace-clippy | 0; canonical runner |
| wasm-workspace-build | 0; canonical runner |
| wasm-exchange-default-build | 0 |
| wasm-exchange-all-features-build | 0 |
| host-workspace-build | 0 |
| source-extractor-check | 0; direct Node/Yarn 1.22.21 |
| workspace-rustfmt | 0 |
| diff-check | 0 |

The serial control runner then exits **0 at the same SHA**, accepted true.
Every mutation compiles, runs exactly one selected native test, fails at its
required assertion with exit 101, restores original source bytes, and passes
the restored test with exit 0. The final tracked tree is clean. No compiler
rejection or unrelated assertion is included. No survivors among the **13
controls attempted**; this finite claim is not complete-function proof.

| Control | Required native assertion | Mutation / restore |
| --- | --- | --- |
| retry-empty-method-default | factory_retry_config_native_empty_method_reentry_methods | 101 / 0 |
| configured-region-passthrough | factory_region_native_construction 3 | 101 / 0 |
| effective-response-hook-config | http_receipt_native_hook_config | 101 / 0 |
| rejected-response-transform | http_transform_native_failure_body | 101 / 0 |
| waiter-immediate-waker-release | shared_native_waiter_drop_releases_waker_immediately | 101 / 0 |
| retry-response-unwind | factory_response_native_auth_failure_unwind | 101 / 0 |
| original-read-token | factory_retry_config_native_attempts private-post-error-method-get | 101 / 0 |
| shared-signing-clock | factory_clock_native_reentry_hmac | 101 / 0 |
| raw-includes-error-kind | factory_transform_native_raw_includes_type_error | 101 / 0 |
| clock-reentry-carried-retry-budget | factory_clock_budget_native_attempts one-used-before-clock-only-two-left | 101 / 0 |
| environment-probe-singleflight | factory_probe_native_concurrent_attempts | 101 / 0 |
| legacy-array-params-own-spread | factory_params_native_legacy_keys unsigned-array | 101 / 0 |
| physical-response-bom | axios_transform_native_value 1 | 101 / 0 |

The fifteen-test target pins 85 request profiles, five construction profiles,
four optional-helper cases and four separate astral refusals. All native
observations precede supplemental comparisons in the aggregate. It preserves
the source's transformed-body signing on retry, even when the physical body
bytes stay unchanged; it does not silently repair that source behavior.

The factory row explicitly retains the source-accepted scalar-astral params
case as an input FINDING owned by this Bybit lane. Other clauses separately
price options snapshots, weak ownership/probe cancellation, original read-token
protection, signer-inclusive retry budgets and the console carrier. The exact
returned D800 string refusal cites the required Value::String surface under
sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary and
sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict; it does not excuse the
astral input finding. Bucket authority is
sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy.

Nine newly introduced behavior items are reconciled into existing source rows
across three manifests. The branch's ten native-carrier/runtime-capability
exemptions remain separate; this reconciliation adds none and updates only the
BybitApi reason to remove its stale pending statement. The pure Bybit retry predicate
is mapped independently from replay policy; existing ApexOmni/Bitrue ownership
remains intact. Static annotation review found no additional concrete predicate
mismatch in its stated typed-input scope. This does not claim arbitrary JS
objects, executable accessors, general Axios equivalence or config-presence
preservation.

Independent sub-agent reviews found no additional production gap within that
scope, verified native-before-supplemental ordering, the thirteen unique control
sites, restoration and required-site checks, child cleanup and applicable CI
compile/lint/build configurations. They are narrow reviews, not parity proofs.
The rejected cdbe sequence above remains rejected; its fourteen zeros do not
replace this complete rerun. No rejected compiled-control sequence occurred in
the accepted thirteen-control run.

Not run: full runtime test suites, live exchange/network exercising, binding
runtime smoke tests, additional mutations and complete-function formal proof.
All compilation/lint/build gates required for this change have run. Complete
workspace structure is reported separately; 24 green gates do not claim that
the inherited canonical diagnostic corpus is green.

The first post-annotation extractor preflight exits 1: three target arrays use
noncanonical text formatting, with zero added or orphaned symbols. The official
reconciler updates exactly those three files; a full parsed comparison proves
all 512 manifests, including every annotation and row order, unchanged. Its
corrected check exits 0. This metadata preflight is separate from the committed
24-gate runtime/CI sequence above; no measurement is reassigned to a later SHA.


### Factory merge reconciliation: reviewed incoming cancellation candidates

Merge c971ebe17279e8fccff0540d0e98f226e78ce412 retains integration
49a942e76143d6d1d44567edff59c12669d92627. Both CLAUDE append conflicts keep
both lanes' blocks. Cargo inputs, factory mutation targets, assertions and
primitive dependencies are unchanged; shared exchange, transport, store and
CrossTrade production changed. The original 24 gates and thirteen controls
remain attributed only to 4b241b5120691effb5449e9afe4e456d5adf5329.

Fresh compiled discovery at c971 exits 0 with exactly one export test: 155
candidates, four added and none removed. Two CrossTrade audit producers changed
without changing their discovered boundaries. Review retains no-owned-latch:
new missing/inherited-symbol branches synchronously report and append ready None
futures; the join owns its children without reserving a shared running slot.
This does not promise rollback of external host work already started.
Four numeric-output expression macros are immediate after expansion and concrete
field-graph review: borrowed primitive/container traversal or empty unit-enum
arms, without serialization, callbacks, futures or lifecycle mutation.

Five guarded rows reference changed whole files: worker_reply and OwnedTasks::start
pin exchange.rs; schedule, spawn_owned_kind and run_reopen_resync pin websocket.rs.
The lifetime implementations, producers and named witnesses are byte-identical;
incoming additions concern synchronous recovery and redaction. Only those five
pins, the two reviewed producer hashes and four new classifications are reconciled.
No owner, guard requirement, witness or boundary is removed. The official validator
preflight exits 0 for 155 classified candidates and zero open findings; this is
not semantic proof of cancellation safety. Compiled classification and affected
positive/CI gates must still run on the committed reconciliation.

At annotation commit 08a87b1c56a2014798e43590a2e48dfaf5c6d5c6, the source
extractor exits 0. Full structure exits 1 on exactly the frozen 5,895 canonical
diagnostics, with zero multiset additions/removals. The scoped real checker also
exits 1; it retains complete source bodies for 17 changed production crates,
a 20-crate dependency closure, 356 projected manifests and 4,626 rows. These are
inherited-red measurements, not green gates. The checker artifact SHA256 is
aec628288714f9d9b877cbc9bb4a5dbb7536bcdc831d7996872829c6ac7a5df2;
its Cargo validation belongs to 4b's host workspace build. New merged structure
must be measured separately. Merge c971's extractor and rustfmt preflights exit 0.


## 2026-10-15 — eight Spot exchange account/order bodies

Code gate SHA: `642c4710c0adece8eead87e72dfbecf2cf4b63a2`.
Actual branch parent: `90f27ee0425468d3ed1a1fdd3df3114eb88ca01a`;
merged integration: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
Own ancestor `bfc4db99f2` restores the previous reviewed Spot emitter/identity
corrections absent from that parent; no unrelated unreviewed lane is an ancestor.

Tomllib comparison against the parent: **8 pending -> ported, 0 -> divergent**.
Assigned exchange manifest: **22/16/11 -> 30/16/3** (ported/divergent/pending),
49 rows. Spot sockets are untouched. Class/constructor/start remain pending;
this slice does not establish concrete production runtime composition.

Measured at the code gate SHA:

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --all-targets --jobs 2`: **0**.
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test spot_exchange_reads --jobs 2 -- --nocapture`: **0**.
  Independent native/source floor **43**, stopped at index **42**, update/mark-zero.
  Native field, alias and original-error assertions precede wire comparisons.
- Scoped all-target clippy with `--jobs 2 -- -D warnings`: **0**.
- `parity:extract:check`: **0**, output says zero updates/additions/orphans.
- Rustfmt check of the new source and native test: **0**.
- Isolated compiled production-body mutation batch: **101**, eight tests reached
  eight named native assertions; no compiler rejection or survivor. The eight
  independent controls and body/fixture blobs are in SPOT_READ_MUTATION_EVIDENCE.json.
  Temporary copies share the production capability trait, each has one body
  mutation, and use the maintained native assertions. Temporary files were
  removed. Restored maintained 43-case target: **0**.

The full crate `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit
--jobs 2 -- --include-ignored` passed **0** at
`4f32b1a1cefd3530e8285a087da358c02f7e883b`, not at the later SHA.
Strict clippy there failed **101** on a custom no-op waker and identical fixture
branches. The code gate SHA fixes only those two mechanical fixture issues.
Scoped `git diff --exit-code` proves production, oracle and every other test
unchanged between those SHAs (**0**); the affected 43-case target was rerun.
The full suite was not rerun after that fixture-only correction.

Earlier development fixture runs failed **101** at candle-elision validation,
date-fixture dispatch, adjusted-price expectation, and virtual-update ID count;
these are fixture corrections, not claimed mutation controls. A disk-full Cargo
attempt also exited **101**; it supplied no passing evidence. Scoped crate cache
cleanup recovered build space before the recorded gates.

Adversarial sub-agent review: `account_read_review`, PR 7848. Initial review
found recording-payload identity, stored-market identity and virtual-error proof
gaps. Fixed and re-reviewed at the code gate SHA; static preservation/census
checks **0**, no remaining blocker identified within the supplied-host scope.
Reviewer ran no Cargo/oracle/extractor/clippy gates. Review citation:
https://github.com/Tealstreet/tealstreet-next/pull/7848#issuecomment-6183282973
The final review at the code gate SHA additionally verified the mechanical fixture-only correction:
https://github.com/Tealstreet/tealstreet-next/pull/7848#issuecomment-6183615573

Not run: workspace build/tests, wasm checks/clippy, CI reruns, or live exchange
requests. The 43-case and eight-mutation enumerations are floors, not complete
coverage of overridden intrinsics, custom prototypes, sparse iteration or all
async scheduling/cancellation behavior. Representation policy remains canonical
in sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary,
sdk-rs/parity/MANIFEST-SPEC.md#closed-typed-output-boundary,
sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias,
sdk-rs/parity/MANIFEST-SPEC.md#numeric-ingress-defect,
sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy and
sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict.


## Factory reconciliation with sprint integration b2d71ee1d37d

The integration merge retains both parents' evidence and test blocks, all eight
Spot promotions, and the factory's one pending-to-divergent/FINDING transition.
The intended Bybit census is 282 ported / 126 divergent / 112 pending; this unit
adds zero ported rows. The scalar astral-param input defect remains witnessed,
not relabelled as parity. Original 24-gate and 13-control acceptance belongs only
to `4b241b5120691effb5449e9afe4e456d5adf5329`.

Shared query rendering gained an explicit ordering profile and a fallible result.
Five factory/receipt sites select Preserve, retaining their previous wire order;
the interceptor propagates render errors. Independent read-only sub-agent review
found both test groups retained without weakened assertions or signing changes.

Integration also moved task registration into start_inner, added detached owner
leases, and changed timer priming and Driver polling. After semantic review,
compiled discovery against the merged working tree exited 0 with 158 candidates:
all 155 earlier keys retained plus start_inner, start_detached and the detached
reconnect reporter. Seven reviewed producer fingerprints and six guard-record
dependency sets were reconciled; no guard or witness was removed. This precommit
discovery is not a committed classification or runtime acceptance measurement.
The guarded registration evidence follows its implementation into start_inner;
start and start_detached delegate there with distinct owner-retention policies.

The delivery sequence reruns the thirteen native factory/receipt gates, the owned
task lifetime target, workspace build, host workspace all-targets clippy and the
canonical wasm clippy helper. Raw SHA-bound receipts remain ignored build output.
No result at the earlier proof revisions is attributed to this merge. This note
records the reconciliation before that committed sequence, not its outcome.


## Rejected/interrupted consumer sequence at 65508f8e5c

At `65508f8e5cffba9d85765453c940566cfed9db3e`, fourteen native gates exited 0
(including factory15 and owned-task26) and workspace build exited 0. Host
workspace all-targets clippy then emitted three E0063 diagnostics: incoming MEXC
client fixtures omitted the new HttpClientOptions response_transform field.
The known-failed keep-going sweep was intentionally interrupted before its
final Cargo verdict; runner exit was 130. No final host-clippy exit is claimed.
Canonical wasm clippy was NOT RUN, the seventeen-gate sequence was NOT accepted,
and zero new mutation controls were attempted. Raw receipts/logs are preserved.

The repair initializes exactly those three test-only options to None. MEXC's
existing ResponseTransform adapter continues to own its response parser and
projection; no production body, assertion or lint policy changes. A narrow
read-only sub-agent review found exactly these three missing fields among
seventeen explicit SDK initializers and approved the None selection.

The repair delivery reruns the seven MEXC client tests, factory15, workspace
build, host workspace all-targets clippy and the canonical wasm clippy helper.
Other native positives remain measurements at 65508f8e5c. The original complete
24-gate/13-control proof remains bound only to 4b241b5120. This disclosure precedes
the repair's measured gate run and does not claim its results in advance.


## Snapshot-refresh timeout foundation, rows still pending

The next unit starts from accepted integration 2b32c7cce3. Source witnesses show
that the debounce timer and already-started snapshot Promise have distinct
lifetimes. The core adds only start_detached_timeout, reusing the existing timer
and Driver with an independent retained registry. Ordinary registry mode stays
false; no detached shared-result API or venue-owned task registry is introduced.

Working-tree preflights: two focused timeout tests and all twenty-eight owned-task
tests exit 0; compiled discovery exports 158 candidates; the established
all-features classification command exits 0 with one executed test. These are
precommit measurements, not delivery-SHA acceptance or controls. Narrow sub-agent
review cleared core ownership and the bounded inventory reconciliation.

The first compilation exited 101: struct update syntax could not move fields
from OwnedTasks because it implements Drop. Explicit field initialization repairs
that draft; no runtime/control ran in the rejected compilation. The queued-runtime
fixture now explicitly observes zero sleeps before poll and one after poll. An
incorrect exact filter executed zero tests, and a default-feature classification
invocation rejected an inactive health witness; neither is an accepted gate.
No assertion or lint policy was weakened.

Inventory reconciliation preserves all 158 owner keys and every prior guard and
witness, adds two timeout witnesses, and updates three reviewed producer hashes.
Eight coarse dependency records change: five owned_tasks pins and three incoming
websocket pins. The latter follow the integration-only handler-count scalar
repair, which changes no creator/guard/timer body. No new venue row is annotated.
The delivery still requires native parity, controls and the four required gates.


## Snapshot-refresh field implementation, pre-acceptance

Two next-source field bodies are implemented; both manifest rows remain pending.
The actual maintained field initializers and both constants run with the installed
Lodash debounce/now bodies. The native implementation reuses SnapshotHost for the
actual receiver, helper selection, await and original reporter error. It does not
claim the full class, constructor or concrete helper binding.

The new corpus has fixed 12 scheduling, 3 reporter, 4 identity/flush and 3 lifetime
profiles. Native times, invocation/result counts, Promise aliases, original error
identity and outcomes are asserted before the source process or any supplemental
comparison. Native/source receivers are now independently asserted. Source timer
rescheduling, cancel retaining its result, flush retaining physical timers,
post-disposal helper settlement and later reporting are preserved, not repaired.

The first native preflight compiled and exited 101 at reporter-reentry timing:
actual 1250/2250 versus the declared 1250/1500. The fixture advanced its clock
before polling a timeout queued during Promise draining. Polling new timer work
after draining repairs the fixture; production and declared times are unchanged.
Subsequent preflights exited 0, but were not acceptance: adversarial review then
found an unobserved selected receiver and a conditional-only reporter rejection
assertion. Receiver identity and all 22 expected raw Promise outcomes are now
unconditional before comparison. Maintained source bodies and declared schedules/values are unchanged;
dependency stubs gained receiver assertions. Production did not change for these
observer repairs, and no row was accepted from the earlier preflights.

Narrow sub-agent implementation and instrument reviews clear the corrected scope.
Both Python runners parse and ten mutation sites match exactly once. No compiled
control has run yet. The committed sequence declares nine serial gates: the four
required CI gates plus canonical wasm build, owned-task28, classification1,
core-snapshot2 and diff-check. SHA-bound raw receipts are ignored output. Neither
preflight zeros nor earlier factory controls are attributed to this new sequence.


## Snapshot sequence withdrawn on the gate-bar change

At 4f937f6a173af8912ca4574f412126285b33c7ab, workspace-fmt, diff-check,
owned-task28, cancellation-classification1, core-snapshot2, workspace-build and
host-workspace-clippy each exited 0. The canonical wasm helper was then stopped
on the explicit fleet instruction to stop local wasm work. Its process exited
-15 and the nine-gate runner exited 1; that declared sequence is WITHDRAWN, not
an accepted nine-gate run. Canonical wasm build did not run, and no compiled
control had been attempted. This is a policy interruption, not a diagnosed
production or instrument failure. The raw receipt and logs are retained locally.

The revised committed runner has seven serial positives: the three required
local gates (fmt, workspace build and host workspace all-targets clippy), plus
diff-check and the same three scoped native targets. Wasm, wiring, layers and
routing now belong to the central integration gate. Ten compiled controls remain
required for these two rows; neither row is relabelled by this workflow change.
No source body, assertion, test floor, lint policy or control was changed.


## Snapshot-refresh acceptance and integration reconciliation

Two rows move pending to ported: debouncedPrivateSnapshotRefresh and
scheduleSetTradingStopSnapshotRefresh. The Bybit tomllib triple changes from
282 ported / 126 divergent / 112 pending to 284 / 126 / 110 across the same
520 rows; the four journal rows are separate. No divergent row changes bucket.
The debounce row maps its state type and four companion methods through one
rs array; its behavior is not hidden in invention exemptions. The class and
constructor remain pending. The already-mapped snapshot helper's concrete class
binding is not established by these field bodies.

Proof revision: ad6d03cf72068efb08833a922e0bbcf89222461c. The committed runner
exited 0 with accepted true and all seven declared positives at 0:

| Gate | Exit | Executed native floor |
| --- | ---: | --- |
| workspace-fmt | 0 | n/a |
| diff-check | 0 | n/a |
| owned-task-lifetimes | 0 | 28 tests |
| cancellation-classification | 0 | 1 test |
| snapshot-source-and-native | 0 | 2 tests, including the new 22-profile oracle |
| workspace-build | 0 | n/a |
| host-workspace-clippy | 0 | n/a |

At that same unchanged revision, all ten compiled controls reached their required
native assertion at 101, restored the exact source bytes, and passed the restored
test at 0. The runner exited 0 with accepted true. No survivors among the ten
controls attempted; compiler rejects and unrelated assertion failures are excluded.

| Control | Mutation / restore exit |
| --- | --- |
| global-timeout-retained-registry | 101 / 0 |
| global-timeout-explicit-cancellation | 101 / 0 |
| snapshot-trailing-delay | 101 / 0 |
| snapshot-max-wait | 101 / 0 |
| snapshot-disposed-guard | 101 / 0 |
| snapshot-original-error-reporter | 101 / 0 |
| snapshot-cancel-retains-result | 101 / 0 |
| snapshot-post-disposal-reporting | 101 / 0 |
| snapshot-selected-helper-receiver | 101 / 0 |
| snapshot-reporter-rejection-retained | 101 / 0 |

Source behavior preserved: cancellation retains the cached Promise and leaves
started work alive; flush leaves already-scheduled physical timers; maxWait and
wall-clock rollback retain Lodash ordering; helper/reporter reentry observes the
correct prior or published result; a helper settling after disposal still reports
its original error. All native result/receiver/time assertions precede supplemental
comparison. These controls cover the named changes, not every possible mutation.

The delivery merge is fef251f8d32a01e41dba7c6eb8e8ae6935e96efd with incoming parent
1810a76a9764020538782a82b29780b98c9778d1. The shared origin ref advanced after the
initial f528 inspection; the additional serde visitor moves and 27 explicit
forwarding methods were reviewed separately. No timeout/snapshot source body, Cargo
manifest or lockfile changed. Shared-core watchdog ownership and worker after-hook errors
did change, so earlier positive gates are not credited to the merged tree.

The compiled discovery export at fef initially exited 101 at its output write:
the relative destination was interpreted from the crate rather than the SDK.
No discovery file was produced. An explicit absolute destination in ignored
storage repaired only the invocation; the unchanged committed exporter then
exited 0 with one executed test. Both logs are preserved. This instrument failure
is not a mutation control or a production finding.

That export retains all 158 owner keys and boundary lists. Semantic review and
the export identify exactly two producer changes, start_watchdog and
call_method_in_worker, plus four dependency-pin updates. Every prior guard,
classification and witness is retained, including the timeout additions. The
watchdog reason explicitly limits its named runtime witness to the Venue branch;
the new Core branch has source review without a new runtime witness here. The
working reconciled inventory validator exits 0; a committed classification result
is not claimed from that preflight. Narrow sub-agent reviews clear these changes.

A separate structural preflight at 4f937f6a17 aborted before indexing completed
on safe-cex-num/src/prototype_realm.rs's thread_local macro. The same binary
returned the identical error against archived unmodified 7bac54167c. No complete
structure pass or zero-new-diagnostic claim is made. Manifest reconciliation
preserves row identities and the source extractor remains a separate check.

The clean annotation commit reruns the seven positives and source-extractor check.
Those results belong to their emitted measured revision, not to ad6 or fef by
inference. The ten controls remain measurements only at ad6. Local wasm work is
intentionally stopped under the revised fleet bar; wasm, wiring, layers and
routing are central integration gates. No push or self-merge is part of delivery.

## Inverse socket dispatch and Store writer bodies

Gate source revision: `01a16f36c12497f9a45719c535cf80086574dafe`.
Integration base: `7bac54167ca1344621ff98cfc492c2838926bffb`.
This revision merges that required formatting-sweep head into implementation
revision `4ee64f5ffd7c74c5bad1e9683f2f1c16085136e6`. `cargo fmt --all` exited 0
and made no further changes. The earlier integration merge is
`94cdf31c6afb8a6f31e9a0040f7449a084c1355f` (base `b2d71ee1d3`); its first parent
retains the previously local, reviewed Coinbase binding work. This deliverable
therefore includes that work as well as the new Bybit module. Its carried
`CoinbasePrivateWebsocket#applyOrderSnapshotBatch` claim is withdrawn from ported
to divergent/FINDING; it is not counted as a pending-row gain.

Tomllib comparison over the two assigned inverse socket manifests: **5 pending
-> ported, 0 pending -> divergent**. Private changes **3/1/7 -> 7/1/3** and
public changes **7/4/7 -> 8/4/6**, in ported/divergent/pending order. The combined
floor is 29 rows and stops at `bybitinverse.ws-public.ts`; nine remain pending.
All eight distinct new targets resolve to real function declarations. Only the
assigned private/public socket manifests were changed for Bybit.

`inverse_socket_bodies.rs` owns the initializer, private message/order/position
bodies and public dispatch. It preserves bare imported-call receivers, current
member functions, captured versus live iteration, in-place order mutation,
fill-before-Store ordering, original thrown handles, shared position pair roots,
captured assignment receivers and UTF-16 topic lookup. `SessionHost` supplies
language capabilities and imported dependencies. Concrete class/runtime assembly,
auth, connection setup and candle/book listener bodies remain separate pending
rows. The existing divergent cold-Future reconnect implementation is not promoted
by the message body's immediate call through the live host function.

The scoped target runs **15 tests**: fourteen named native witnesses and one
aggregate. The aggregate runs all fourteen native witnesses before starting the
actual TypeScript oracle or making wire comparisons. The source floor has nine
literal case identities, stopped at `pong`; the status matrix has eight literal
expectations, stopped at `Unknown`. Native assertions inspect values, IEEE bits,
receiver/array/function/error identity and committed native Store fields. Order
and position failure witnesses use the real fallible `DefaultStore`, retain its
original subscriber cause, prove committed state, and independently assert that
later fanout, rows, clock or timestamp work was skipped.

The primitive iterator witnesses establish rejection before property reads,
body evaluation or close, plus original body-error precedence over close errors.
They do not establish native standard-error class metadata or engine-specific
built-in error wording: the finite realm uses opaque diagnostic tags for these
language errors. Original subscriber and callback error roots are independently
asserted. The realm is a finite witness, not an exhaustive JavaScript engine or
a proof over all BigInt/coercion/prototype/sparse-array/scheduling combinations.

At implementation revision `4ee64f5ffd7c74c5bad1e9683f2f1c16085136e6`,
six compiled production mutations each reached their named native assertion
(exit 101), then passed after restoration (exit 0). The independent iteration
stopped at `iterator-object-validation`: initializer-double-factory,
private-retry-boundary, order-inverse-price, position-pair-alias,
public-prefix-drop and iterator-object-validation. Baseline and final baseline
are exit 0; original source hash is restored; scoped diff is exit 0. No survivor
or invalid control occurred in this six-control batch.
The two carried Coinbase arrow controls were also measured there: each reached
its named assertion (101), then passed restored (0), with no survivors. A scoped
byte diff from that revision to the gate source revision exits 0 for the new
Bybit source, test, realm, source oracle and control script; separate byte diffs
for Coinbase `class_methods.rs` and its arrow control script also exit 0.
These control results remain attached to their actual measured revision.

Adversarial native sub-agent review inspected source, tests, source oracle and
mappings. Its iterator-result validation finding was fixed and paired source/
native witnesses added; final static review found no remaining blocker in this
body scope. Reviewer ran no Cargo, Node or mutation gates. Constructor/class
promotion is not part of that review claim.

A focused sub-agent review of integration merge `01a16f36c1` independently
parsed both parents and the resolved exemption file: 1,758 / 1,724 entries,
1,705 identical overlaps, and exactly 1,777 unique merged entries, with neither
parent losing or changing a record. It verified the inverse module registration,
carried lazy signing helpers and wrapper dispatch survive the source auto-merges.
No material merge blocker was found. This was static review, not an executed gate.

The reconciler WRITER reported `Updated 2 manifests; 0 added, 0 orphaned symbols`
and exit 0. It preserved existing annotations. Extractor check and other static
report checks were not run under the sprint bar.

Boundary references remain canonical:
[14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy),
[19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Measured serial gates at the gate source revision above, with the installed `1.99.0`
toolchain (`rustc 1.99.0 (b940084d7 2026-09-28)`), host `aarch64-apple-darwin`,
and two Cargo jobs:

| Gate | Exit | Native floor / scope |
| --- | --- | --- |
| Coinbase `completion` and `signing_policy` tests | 0 | 8 + 2 tests |
| Core `default_privacy_uses_dynamic_signing_getter` | 0 | 1 test, 13 filtered |
| Bybit `inverse_socket_bodies` native/source target | 0 | 15 tests |
| `cargo fmt --all --check` | 0 | workspace |
| `cargo build --workspace --locked --jobs 2` | 0 | workspace |
| `cargo clippy --workspace --all-targets --locked --jobs 2` | 0 | host workspace, all targets |
| `node ../.github/scripts/rust-wasm.mjs --clippy` | 1 | interrupted on explicit user gate-bar change; wrapped Cargo 255 |

The receipts and named mutation logs are tracked in
[evidence/inverse-socket-bodies](evidence/inverse-socket-bodies/code-gates.json).
Development attempts include two interrupted development runs and fixture compile failures;
none is counted as a pass or a discriminating control. The second interruption
was premature because dependency compilation had resumed.

The earlier `4ee64f5ffd` workspace build and host clippy exited 0. Its wasm
helper was deliberately stopped for the required integration merge: helper exit
1, wrapped Cargo status 255 after SIGINT. That attempt is not a wasm pass or a
compilation defect; its receipt is preserved separately.

At `01a16f36c1`, the wasm attempt was also stopped by explicit user instruction:
wasm clippy is now a central integration gate, removed from the lane delivery bar.
Its helper exit 1 wraps Cargo status 255 after SIGINT. No local wasm pass is claimed.
The required lane bar is fmt, workspace build and host workspace clippy.

This evidence commit changes documentation and receipts only. Before reporting
its final SHA, rerun the three required workspace gates and the 15-test target there; keep
that measured receipt under `.aimux/inverse-socket-handoff/<final-SHA>/` and prove
source/tests/oracles/manifests/lockfile unchanged from the gate source revision. The
mutation numbers above belong to the implementation/control revision, not the evidence commit.

Not run under this sprint bar: standalone workspace `cargo check`, workspace
tests, full Bybit suite, extractor check, parity static/report checks,
CI-wiring/layer/routing checks, wasm build, core all-features and FFI feature
matrices/smokes, generated TypeScript and full crypto/venue/registry/store/worker oracles, numeric
observations/shared primitive tests, CI reruns or live exchange requests. Local build/clippy is not a green remote
Rust / TypeScript parity job: the integration observation receipt records its
actual queried status. No push or self-merge was performed.

## Snapshot delivery merge after the first green handoff candidate

At 0e7011fc1a67932496fe45ab61c13e91c85b9ef0 the seven-positive runner
exited 0 with accepted true. Each named gate exited 0, with executed native
floors 28 / 1 / 2; the source extractor separately exited 0. Host workspace
clippy took 1393.60 seconds. These are measurements at that revision only;
the ten controls remain measurements at ad6d03cf72068efb08833a922e0bbcf89222461c.

Pinned integration 53ee502232b005173d8a6f3f0d7cf94bad7f072c was then merged
as 8be1cf519249a92c794a8ca67d90e967c27e1dbd. Both module exports, complete
evidence append blocks and exemption records survive the three conflicts.
Sub-agent review independently confirms the exact 1801-key exemption union
from parents 1725 / 1800, with no changed or dropped parent record. No manifest
was regenerated. Against that incoming integration, exactly the two snapshot
rows move pending to ported: 287 / 126 / 107 becomes 289 / 126 / 105 across
520 unchanged Bybit keys; the four journal rows remain separate. The incoming
five inverse-socket gains are retained and are not credited to this slice.

The merge-range whitespace check exits 2 on inherited generated logs and an
incoming document; the authored diff against pinned integration exits 0.
The initial staged check's individual exit was not captured; those two later
read-only checks are the explicit exit measurements. Incoming receipts remain
unchanged. This does not claim a whitespace-clean diff from the older parent.

Compiled discovery at 8be1cf519249a92c794a8ca67d90e967c27e1dbd exits 0
with one executed export test. All 158 keys, boundaries and producer tokens
are identical to the prior fef export. The incoming signing-predicate extraction
preserves lazy evaluation and leaves the worker guard, Base disposal, registry,
Driver and named witness bodies unchanged. Exactly two coarse exchange.rs
pins, worker_reply and OwnedTasks::start_inner, are reconciled with bounded
review reasons. Every guard, witness, classification and delegate is retained.
The Node validator exits 0 on that reconciled working inventory; this is not
a compiled classification result at the merge revision.

The clean pin/evidence commit reruns the seven positives and source extractor.
Results belong to the revision emitted by those runners. No result is attributed
to that new tree in advance. No local wasm, wiring, layers or routing run is
part of this delivery; those gates remain centrally owned. Full class and
constructor composition remain pending. No push or self-merge was performed.

## Rejected merged candidate: module ordering

At 3b2e050f8203e802f297bdec3bea3a9a242f65f8 the restarted runner exits 1
at its first gate, workspace-fmt exit 1: rustfmt requires inverse_socket_bodies
before private_snapshot_refresh in their shared declaration group. No native,
build, clippy or control ran in that sequence. The separate extractor exits 0
at that revision; this is not seven green gates or delivery acceptance.
Running cargo fmt exits 0 and changes only that declaration order, retaining
both modules. Production bodies, assertions, mappings, lint policy and controls
are unchanged. The new committed repair restarts all seven positives; the old
0e positives and ad6 controls remain attributed only to their measured revisions.

## Snapshot delivery reconciliation with sprint batch

The accepted bb7e47ac273b6daf0cb662521388de2afb8cdf71 delivery was held for
its shared-core living-spec conflict. This revision merges the requested
ed8a613e1c09b49c2cf0b3864efef28801c78996 batch head. The detached-timeout
ownership paragraph, eager signer/income entrypoint statement and managed
start_closed contract reference describe separate invariants; each appears once
and both parents' substance is retained. Independent narrow reviews found no
contradiction or duplicate invariant. No TypeScript or snapshot body changed.

The cancellation inventory retains all 158 previous candidates and the three
incoming dormant/open-connection candidates, with every existing guard and
witness retained. The merged owned-task implementation is unchanged from bb7;
its immutable registry mode keeps ordinary socket tasks disposal-owned. The
incoming dormant activation's coarse dependency pin is reconciled to that
implementation after semantic review. Five exchange/socket dependency pins
follow the incoming eager signer and socket changes. Reopen resync really adds
a self-woken pending yield, with its guard live across that point: its producer
and boundaries change, and the inherited unchanged-producer sentence is repaired.
Incoming eager income paging retains its first request in the returned future.

The next slice branch's earlier local merge of aab488a8e2ac3a06ad1f53777831d21659aac289
was completed before returning here; the relevant core objects are identical
between that head and the requested batch. This delivery uses a direct ordinary
merge of ed8, not that next slice's history. All measurements at bb7 and all ten
controls at ad6d03cf72068efb08833a922e0bbcf89222461c remain bound to those
revisions. The required three local gates and scoped positive checks are rerun
after this committed reconciliation; their raw SHA-bound receipts are ignored.
No fresh controls or local wasm measurements are claimed by this reconciliation.

## Primary local-order bodies: preparation, not acceptance

The next three source rows are reconcileLocalReduceOnlyLimitOrder,
recordRecentlyPlacedOrderFromOpts and applyLocalOrderUpdateAfterRestWrite.
No row is relabelled by the preparation described here. Class and constructor
composition remain separately pending.

The primary and Spot record-from-options initializers are byte-identical after
selecting their explicitly named classes. The first convenience probe selected
the first class declaration and exited 1 at its field-count assertion; the
corrected named-class probe exits 0. Sharing the existing Rust function requires
both manifest row notes to explain the collapse under MANIFEST-SPEC section 8.
No separate alias or invention exemption is required for that function.

Source-only preflights execute the actual maintained field initializers. The
reconciliation feeder retains eighteen fixed profiles; the write feeder retains
56 quantity/price cases, ten option supplements and ten update cases. Native
fixture drafts require all native observations before launching either source
process. Native compilation, controls and acceptance are not inferred from
these source and formatting preflights.

Static review found a missing template intrinsic in the reconciliation fixture;
its forwarding host would have panicked on the first ordinary rejection. The
fixture now converts the observed primitive inputs while retaining UTF-16 and
visibly rejects unsupported object conversions. No runtime failure is claimed
for that static prediction. Source collaborator arrows were changed to normal
functions with actual receiver assertions. A separate dependency-fixture defect
supplied the label Warning rather than the maintained value warning. The source
now executes the real LogSeverity module; native setup uses the core enum and
independently asserts warning before observing the argument. All eighteen source
case identities remain; the production field body and corpus inputs were not
changed to fit those observer repairs.

The reconciliation warning observer projects UTF-16 code units before JSON,
including a lone-unit error message and raw-zero rejection. Numeric supplemental
comparisons use binary64 bits. These repairs were made before any native or
mutation acceptance; they are not claimed as previous runtime failures.

The write fixture wraps the existing host rather than changing earlier tests.
Its new dependencies model array string conversion, the own-null-toString
TypeError, NaN-preserving Math.max, the real enum spellings and the fractional
clock. Its finite input profile is not a complete object-coercion implementation.
Native assertions inspect original-filled remaining values, fresh and retained
objects, exact receivers, unchanged inputs, map identities and fixed counts.

The initial control-declaration loader used separate exec namespaces and exited
1 with NameError before any mutation. One namespace repaired that inspection;
the Python AST and seven unique production sites then validate. This is neither
a compiled mutation nor a runtime control. Five reconciliation controls and two
write controls are declared. Their first expected native assertions were reviewed
in profile order, including the strict grace comparison first failing at the
below-tolerance profile, before any later dedicated grace profile.

A separately labelled dependency profile executes an owned-VM strict wrapper
around actual parseFloat. Both source calls have undefined receiver and retain
the original qty and price inputs; the original 76 observations are unchanged.
The new native fixture now supplies a raw callable and independently requires
that bare receiver, before invoking Node. The existing shared Rust parse helper
still uses member selection at this preparation stage; its predicted native
failure is not a measured runtime result. An eighth compiled control is prepared
to restore the member selection only after native reproduction and repair. No
existing row status, production code or native gate was changed by this probe.

## Native bare-call baseline and shared repair

At committed fe453ffcb75e1e38627052c13c53d79d9dfd9ee5, the exact primary
write oracle compiled and exited 101 at local_write_native_parse_receiver on
the first matrix case, before any Node invocation or supplemental comparison.
This reproduces the source-witnessed mismatch: the shared Spot helper selected
parseFloat as a global member although the maintained initializer makes a bare
call. This baseline failure is not a compiled mutation control, accepted gate
sequence or post-control restoration. No production bytes were temporarily
mutated by that baseline invocation. All three new row annotations stay pending.

The shared helper now gets the raw callable before evaluating its argument,
preserving source callee capture and bare receiver. The existing Spot fixture
supplies the actual raw callable and now independently requires its undefined
receiver; its assertions are strengthened, not bypassed. The new fixture retains
all 76 baseline profiles and its separate strict dependency profile. Only this
private parser helper calls the changed operation in spot_order_writes; the
Spot read fixture implements the host trait but does not call this private helper.
The gate runner includes both existing Spot regression tests in addition to the
new oracles and three required local CI gates. Acceptance and eight compiled
controls remain unmeasured at this repair step.

A formatting convenience invocation from the repository root exited 1 with
Failed to find targets. Running cargo fmt from sdk-rs exited 0 before this
commit; the failed invocation is not a declared gate or mutation control.

## Local order reconciliation acceptance

Proof revision 1c3ef9fc27f9a88ccae70914fd52069f5f8c0988 completed all seven
then-declared gates with runner exit 0 and accepted true: workspace-fmt 0,
diff-check 0, local-reconciliation 0 (one test, 18 profiles),
primary-local-order-writes 0 (one test, 76 original profiles plus one separate
strict parse receiver profile), existing-spot-order-writes 0 (two tests),
workspace-build 0 and host-workspace-clippy 0. The source extractor separately
exited 0 with zero manifest updates, additions or orphans. These measurements
belong to that proof revision, not a later delivery revision.

Eight compiled controls completed there, each reaching its required first native
assertion with exit 101, exact production restoration, and restored test exit 0:

| Control | Required first native assertion | Mutation / restore |
| --- | --- | --- |
| reconciliation-tolerance | reconciliation_native_seen_floor tolerance-boundary | 101 / 0 |
| reconciliation-strict-grace-boundary | reconciliation_native_orders_floor below-tolerance | 101 / 0 |
| reconciliation-disposed-after-fills | reconciliation_native_seen_floor disposed-after-fills | 101 / 0 |
| reconciliation-logger-error-retention | reconciliation_native_rejection_floor | 101 / 0 |
| reconciliation-order-predicate | reconciliation_native_predicate_false_then_true | 101 / 0 |
| record-opts-bare-parse-float-receiver | local_write_native_parse_receiver | 101 / 0 |
| record-opts-conditional-clearing | local_write_native_reduce_only conditional-clear | 101 / 0 |
| local-update-original-filled | local_write_native_original_filled_remaining amount | 101 / 0 |

No survivors among the eight controls attempted. Compiler rejects and a 101 at
an unrelated assertion are excluded. Two independent reviewers checked the
control logs and annotation draft; full-target tokens, rs array spacing and
per-row corpus counts were corrected before applying annotations. No failed or
rejected committed sequence occurred in this unit; the earlier baseline failure
and formatting usage error remain separately disclosed above.

Three primary rows move pending to ported: reconciliation, record options and
local update after REST write. The existing Spot record row receives only the
shared-target explanation, with zero new credit. All 520 file-plus-symbol keys
are retained: own slice baseline 289/126/105 becomes 292/126/102. Previously
accepted snapshot gains and incoming inverse-socket gains are not this slice.
The reconciliation oracle observes 18 native cases before supplemental Node
comparisons; the write oracle observes all 76 original cases and its separate
dependency profile before any Node comparison. Record options owns 66 original
cases plus that dependency profile; the update row owns ten original cases.
Finite fixtures do not prove every possible JS coercion or concrete class binding.

Origin 0286f63437 merged at ba63d9e9a7ec2fb700836ffa2d051088715ee20d.
Bybit Rust and maintained TypeScript, shared exchange code, Cargo.lock and
workspace Cargo.toml are unchanged against the proof revision. Incoming retained
JSON and transport behavior changes still require delivery runtime checks.
Latest fleet policy requires formatting and locked workspace build locally;
clippy, wasm, wiring, layers and routing are centrally owned. The local-order
runner now omits clippy and retains its three scoped native gates, formatting,
diff check and build. The snapshot runner is historical and is not rerun here.
Controls are not replayed or attributed to delivery. No remote push is performed.

## Startup extraction checkpoint, not accepted

The previous three-row local-order delivery is frozen at
5ee8bc62283b550dfbe8635387773c9f73dc47d5 with its exact-SHA six positive
gates and two required local CI exits all 0, and was handed off without a push.
This new branch starts its next pending function, BybitExchange.start. The
prepared source corpus retains 33 original profiles plus three independently
labelled evaluation-order supplements. No native result, mutation control,
annotation or accepted startup gate is claimed at this checkpoint.

The source cache continuation is empty because fetchAndCacheApiKeyInfo normally
logs its own failures. Its own final logger can reject, or a supplied method can
reject directly; start's empty continuation suppresses that rejection. The Rust
callback instead sends the retained original error value to emitAndLogError,
and retains any reporter failure. This is an intentional visible-error
correction, not claimed parity or a measured defect at this preparation stage.
The success continuation for margin probing remains distinct. A later paired
source/native witness must establish the correction before changing the row.

The hard stop arrived during the startup native-host fixture implementation.
That unfinished host is preserved only in ignored local preparation, unmounted
and uncompiled; it is not an accepted oracle or part of the delivery commit.
No startup mutation ran, and its row remains pending with zero new row credit.
Final checkpoint formatting and workspace build must be measured against the
checkpoint SHA separately from the accepted three-row local-order delivery.

## Spot production facade and registry (ddi2il-18, first variant)

`tests/spot_binding.rs` constructs `BybitExchange::create(Bybitspot, ...)`, then
calls the concrete Exchange trait and typed core preflight over scripted HTTP
and real core socket orchestration. Five E2Es pin spot/1x market mapping,
tickers, account and validation/balance, empty positions with no position query,
order reads/placement/batch/amend/cancel, exact inherited refusals, public/private
startup, loaded flags, order-only private subscribe after auth, both socket
closures and Brazil's local order retirement. Caller IDs are sent verbatim;
invalid and split IDs reject before any write.

`python3 sdk-rs/safe-cex-venue-bybit/scripts/spot-binding-controls.py` compiled
17 independent trait-consumer mutations: start, get_account, validate_account,
fetch_positions, fetch_orders, fetch_markets, fetch_tickers, place_order,
place_orders, update_order, cancel_orders, set_leverage, change_position_mode,
change_margin_type, fetch_symbol_max_leverage, schedule_cancel and
fetch_public_trades. Each failed an executed assertion, with no compiler error;
source restored byte-identically and all five Spot tests passed. The runner
writes temporary output under `/tmp`, never repository evidence artifacts.
Removing the Bybitspot RUST_VENUES entry separately compiled and failed the
exact registry membership assertion, then the entry was restored.

The existing source-driven Spot read/write and variant account-refresh oracles
remain separate from these production construction proofs. The source class is
carried by the discriminated Rust `BybitExchange` type with a distinct Spot
receiver and class_name; that API representation choice is declared on its
class/constructor rows. No Inverse or HyroTrader constructor claim is made here.

## Production private event delivery (ddi2il-27)

The private socket previously used WsProtocol's unimplemented message default.
`PrivateProtocol::on_message` now delegates to a shared retained receiver consumer
in `ws_private/messages.rs`; the constructor owns auth retry state and one fresh
canonical `create_incremental_fills(500)` tracker. The same consumer calls the
variant's actual mappers and preserves the dedicated Spot/Inverse branch rules.
Primary and Demo are the currently constructible receivers; the remaining
variant constructors and Hyro session wrapper remain separately mapped work.

Six tests in `tests/private_event_delivery.rs`, run with `--include-ignored`, pass:

- real socket-pump order delivery, parent/virtual rows, duplicate and advancing
  cumulative fills with independent numeric assertions, terminal publication;
- positions, balance upnl, default category rejection, zero-contract updates,
  pong statistics, disposal, and source-ignored execution/wallet frames;
- opted-in inverse order units and false replacement partials;
- bounded auth retry starts (four, matching the source <= 3 guard);
- actual placement -> omitted REST snapshot -> private order sighting -> next
  snapshot, proving the sighting releases recently-placed snapshot grace;
- 24 actual-source compositions: four option/inverse switch combinations times
  six ordered frame scenes. The oracle extracts the maintained private members,
  actual exchange mappers/selectors, canonical fill helpers and DefaultStore.
  Native Store and fill snapshots are independently compared after every frame;
  each side asserts a fixed 24-case total. Host I/O is the scripted boundary.

`python3 sdk-rs/safe-cex-venue-bybit/scripts/private-delivery-controls.py` compiles
six mutations: live message dispatch disabled, category gate bypassed, fresh fill
cache on every frame, missing markOrderSeen, discarded position publication,
and auth retry boundary changed. Every mutation fails its named runtime test;
no compiler failure counts as detection. Source bytes are restored and all six
tests pass afterward. The runner stores logs only under `/tmp`.

The production E2Es cover primary/Demo, including opted-in inverse/option rows.
No complete dedicated Spot/Inverse/Hyro constructor or session-gate claim is
inferred from this handler. Binary frames remain a declared SocketFrame input
boundary; text frames use shared JSON.parse and catch only parse failures.

Override call-site review: exactly 13 existing references in receiver::install
keep their original targets. Their enclosing fingerprint changes from
711c716ab03d25ff to 8adb93f50b6745a7 solely because tradesCategoryOf is registered.
Deleting just that three-line registration reproduces the previous receiver
file byte-identically. The selector dynamically calls the already-bound
category/option/inverse members; it does not capture a BaseExchange method.
No delegation classification or bypass exemption is added.


## Spot facade and registry composition

Production checkpoint `2fb414e5e1`, witness/mapping checkpoint `86e247c10e`:
`exchange_binding` 22, `option_binding` 1, `private_event_delivery` 6 and
`spot_binding` 6 all pass, including ignored live source comparisons. The primary
order-write oracle retains 92 cases; private delivery independently asserts 48
source/native compositions (two actual handlers/mappers, four flag modes, six
scenes). Spot private frames publish live orders and replay-suppressed fills;
position/execution/wallet topics retain the source ignores. Valid-array corpus
coverage does not establish malformed-frame error-message parity.

The serial Spot control runner compiled and caught 17 trait consumer mutations
plus removal of the early source isSpot initializer. It restored exact production
bytes and all six Spot E2Es passed. The typed base now receives is_spot=true before
Rc publication through the single shared factory from #233; no venue-name policy
is placed in core. Unsupported source BaseExchange hooks retain their exact
Not implemented rejection, including max-leverage lookup, public trades and
schedule cancel; market metadata separately advertises Spot leverage 1.

The override inventory review retains all existing references and adds only eight
Bybit references: six actual Spot helper-body delegates plus emitter name reads
for order-only subscriptions and Spot ticker projection. The 13 existing receiver
references retain their targets; only its enclosing install body changes when
Spot members are installed. Account validation keeps the primary consumer while
selecting the existing derived validator for Spot. No delegation exemption is
added, and foreign inventory rows are preserved.

At the Spot checkpoint, FINDING ddi2il-38 recorded the unwired Spot active
selector, change callbacks and retained topics. The separate public-topic
checkpoint below repairs that composition. Remaining dedicated Inverse/HyroTrader
constructors and unrelated public class members stay separate claims.

Final scoped gates: native Bybit registry 3 passed (local/hosted signing and
untouched demo origin), registry constructors 14 passed, core cancellation
inventory 6 passed plus one explicit export ignored, override contract 4 passed
plus one existing live-source inventory ignored. Host all-targets and wasm library
clippy for Bybit/registry passed. Parity reports 7459 rows / 17145 public items
with zero findings; extractor reports zero changed/added/orphaned. Registry-data
and layers checks pass. The 1073 existing override entries are retained and the
eight reviewed Bybit entries give a total of 1081; the incoming eight-row Binance
repair is the owner's commit 04bfb16ccf, not an independent reclassification.


## Concrete public ticker subscriptions (ddi2il-38)

Production checkpoint `1fb65d0b57`, with witnesses/manifests in `eaad2472d4`.
`PublicProtocol` now composes the existing topic and lifecycle bodies through
`ws_public/topics.rs`. Source policy differs deliberately: primary linear and
inverse sockets subscribe every non-option market in their category, while Spot
calls its live parent selector and retains the union of previous/desired topics.
The primary class has no active-symbol callback. Spot registers that callback in
construction before any dial; all profiles replace their own markets callback
on open. Source duplicate market additions and absence of unsubscribe are kept.
The canonical subscribe and Spot market-addition bodies now execute their first
send and sleep factory eagerly, preserving first-chunk/ping/later-chunk order.
Core owns the remaining tails; listener/tail captures keep the venue/socket weak.

Four concrete tests in `public_topic_binding` pass. Two assert physical wire
frames for primary category/option filtering and Spot pre-dial, live/offline
active changes, reopen retention, duplicate market additions and disposal. A
third runs exactly 24 maintained-source constructor/handler/parent-selector
compositions (three connection profiles by eight initial active states), with
15 snapshots each and independently counted native executions: 360 snapshots.
The fourth disposes during each pending initial-subscription/market-addition tail
and requires no later send or retained venue. Malformed executable market objects
are outside this typed native Market emitter profile; the full public OHLCV,
order-book and recovery-timer class bindings remain pending.

`scripts/public-topic-controls.py` compiled six production faults: omitted initial
sync, missing constructor active registration, missing live active send, missing
markets registration, discarded retained topics, and reversed primary option
filtering. Every fault failed its named runtime assertion; compilation failures
were rejected. Exact production bytes were restored and the positive tests pass.
The initial-sync fault is caught by the source-composition test (the standalone
Spot scenario had already synchronized its topics before open). The original
public_ticker_topics/public_ws_lifecycle live helper targets also pass, separately
from these actual facade consumers. The integrated include-ignored run passes
primary binding 22 (including 92 actual-source order-write cases), private delivery
6 (including 48 source compositions), Spot binding 6 and both live public helper
targets. Shared cancellation inventory is 6 passed/one export ignored; override
contract is 5 passed/one existing live inventory ignored. Bybit host all-targets
and wasm library clippy pass. Global parity is zero (7459 rows/17162 public items);
extractor check reports zero changed/added/orphaned manifests.

The override inventory retains all 1101 incoming rows and adds one reviewed
Bybit reference, `self.topics.dispose()` inside `PublicProtocol::dispose`: the
core protocol cleanup calls the source Spot active-listener removal
(`packages/safe-cex/src/exchanges/bybit/bybitspot.ws-public.ts:144-147`,
`BybitPublicWebsocket.dispose`) before core
socket cleanup, with no BaseExchange dispatch bypass. Six existing Spot helper
references keep identical targets; their enclosing installer fingerprint changes
from `d269ecf6e0eb661f` to `0858624e3dde1a3b` solely because the two source selector
members are installed there. No foreign row or delegation exemption is changed.

## Primary batch receiver (ddi2il-40)

The real `BaseExchange.place_orders` -> `Exchange::place_orders` -> JS receiver
entrypoint previously failed with `object.placeOrders is not a function` before
any HTTP request. `primary_batch_uses_source_batch_endpoint_regression` failed
on the old receiver and passes on the new `/v5/order/create-batch` path.

`primary_batch_binding` has five physical transport tests. Its fixed 24-case
source composition runs the actual TS `placeOrders`, `placeBatchOrders`,
`placeBatchOrdersForCategory`, `processBatch`, `formatOrderForBatch`,
`getOrderExecutionParams`, `getOrderPositionIdx`, category selector and venue
error formatter. It imports maintained execution controls, symbol classifiers,
math and object helpers and the installed Lodash chunk implementation. Only
HTTP, position-mode input and host recording are scripted. Native requests,
returned IDs and diagnostics agree; native cache results are observed again
through `fetchOrders`. The corpus covers 0/1/2/10/11/21 orders, per-request TP/SL,
conditional limit/market and missing/rounded-zero triggers, caller IDs, hedged
buy/sell/reduce-only indices, missing middle markets, partial agreement errors,
and first category order. Its source-only record/sleep arrays are diagnostics,
not a claim that raw native objects alias JS source objects.

Separate real-facade cases cover whole-batch rejection before any HTTP, batch
failure followed by a failed and a successful single write, special orders
before regular batches (BBO and options plus a failing trailing stop), and
cancellation during a pending inter-batch delay. Primary 23 and Spot 6 tests
pass, including the existing 92-case maintained-source order-write comparison
when SAFE_CEX_TYPESCRIPT_PATH names the installed compiler.

`scripts/primary-batch-controls.py` compiles eight consumer faults: missing
receiver installation, missing atomic validation, missing 1200ms delay, missing
result IDs, missing cache publication, discarded formatter output, BBO routed
as a regular batch, and stripped TP/SL. Each must fail a named runtime assertion;
compiler failures do not count. Exact source bytes are restored and the five
positive tests pass. Host all-targets and wasm library clippy pass.

The reviewed override inventory retains all 1136 incoming entries, with no new
call, removed call or delegation exemption. Exactly 13 references in the primary
receiver installer retain their callees and change enclosing fingerprint from
`10c6c5db2f90d512` to `0045656008997cb5`. Removing only the new batch installer call
reproduces the complete base receiver file byte for byte. The new module installs
live JS members and calls no BaseExchange override directly; Spot still replaces
its own methods after common installation. Shared cancellation inventory passes;
its creator classifications are unchanged.

The async native profile retains the shared typed options/Store snapshot and
UTF-8 boundaries, and Scope cancellation stops tails that a source Promise can
continue after disposal. No arbitrary executable option getter/mutation claim is
made. The dedicated Inverse formatter/submission and HyroTrader facade remain
separate work; this change enables primary Bybit and Demo batching only.

## Native leverage and position-mode publication (ddi2il-42)

Two real-facade regressions fail on the former receiver adapters. A fetched
position with `leverage: "NaN"` reaches the native model correctly, but the old
cached-position JSON projection changes it to null, defaults current leverage
to one and skips a requested update to one. Separately, placement's
`fetchPositionMode` maps that same row, projects NaN to null, and fails typed
Store publication with `invalid type: null, expected f64` before order creation.

Both receiver bodies now retain shared `Js` values through selection, mapping,
comparison and Store publication. No serializer bridge is introduced. The
leverage body still refuses options before lookup, clamps at the live market
limits, uses strict change detection, accepts the primary not-modified responses,
and refreshes after an accepted write. Mode lookup preserves first cached mode,
array-only response lists, source proxy-status diagnostics, nonempty-list
failure override and the first mapped mode. Its private retained status helper
specializes the maintained response helper's four consumed status fields; the
separate legacy JSON APIs keep their existing declared input/output profiles.

`exchange_binding` passes 28 tests with installed TypeScript, including the
existing 92 source order-write cases and a new fixed 32-case comparison of the
maintained `setLeverage`, `fetchPositionMode`, `mapPosition`, category and refusal
bodies. The new source runner imports the maintained response, symbol, get-key,
ID and enum helpers. I/O and outer Store publication are scripted. Independent
native assertions inspect NaN and negative-zero bits before any output encoding;
the source comparison encodes those scalars explicitly for transport. Cases span
Bybit/Demo, cached/fetched positions, NaN/infinity/-0/equal-one, request payloads,
cache bypass and final Store values. Eight native response cases separately
witness numeric/string/proxy/absent status, malformed lists and nonempty failure
rows. Accepted/rejected not-modified writes and option refusal are also native.

`scripts/native-position-controls.py` compiles four JSON projection faults:
current leverage, initial fetched publication, accepted-write refresh and mapped
mode publication. All four fail named runtime assertions; compilation failures
do not count. Exact source restoration passes all 28 tests. Host all-targets and
wasm library clippy pass. Parity is zero (7459 rows/17183 public Rust items), and
the extractor reports zero updates/additions/orphans.

The override inventory changes from 1136 to 1135 rows, removing only the former
`controls::set_leverage` reference in the replaced receiver function (fingerprint
`444f3b058e699912`). No new/changed reference, foreign row or delegation exemption
is added. The source body now executes directly on its receiver; the standalone
legacy helper remains independently mapped. Scope still owns asynchronous tails;
shared typed Store snapshots and UTF-8 boundaries remain explicit host choices.

A read-only cross-venue census found no other leverage adapter with the exact
cached-positions `serde_json::to_value` pattern. ApexOmni's private position
projection immediately restores native scalar fields with ModelPatch; Backpack's
Market snapshot projections are a broader candidate outside its leverage setter,
without a proven failure in this work. No foreign production file is changed.

The mandatory full `venue-oracle:bybit` command passes 204 tests across 49 test
binaries (42 contain tests; seven contain zero), with zero failures or ignored
tests. It runs with `SAFE_CEX_TYPESCRIPT_PATH` pointing at the installed compiler.
Latest master `efbc4969b6` merges cleanly: the incoming Asterdex inventory reviews
are preserved alongside the single owned Bybit removal. No Bybit or shared
runtime production body changes in that integration.


The native position control runner bounds every Cargo subprocess to 600 seconds
and prints an independently created private `tempfile.mkdtemp` directory for its
logs. An injected TimeoutExpired after the first real source mutation propagated
through the runner while its finally block restored both Rust files byte for
byte; the created directory's mode was 0700. The four actual compiled projection
controls were then caught again and exact restoration passed all 28 binding
tests. A read-only review of these runner changes reported no findings.

## Dedicated Inverse receiver and native registry binding

`BybitExchange::create(Bybitinverse, ...)` now installs the dedicated retained
`BybitInverseExchange` receiver. Its typed source flags are false before the Rc
is published. The existing registry host, hosted signer and caller-owned limiter
construct it without a second host implementation. `native_host_bybit` passes
all three tests; local and secretless-hosted loops include Inverse. Removing only
the registry entry compiles, fails both constructor assertions, and restores to
three passing tests. Registry membership passes all 15 tests.

`inverse_binding` passes nine concrete trait/socket tests: account/validation,
reads and all trading methods, exact inherited refusals, constructor identity,
USD sizing/coin cache units, Brazil amend/cancel, source mode/margin behavior,
strict leverage rejection, native NaN/-0, startup socket selection/auth,
active-topic union and both callbacks, disposal/weak ownership, and physical
private order/fill/position delivery. Private positions update existing rows
only; their NaN leverage survives the landed shared Store patch bridge (#296).
Execution and wallet frames remain source no-ops. The old constructor rejection
fails the native witness before any implementation is admitted.

`inverse_batch_binding` runs 36 independently counted actual-source batch/cache
compositions against the physical native transport. It executes maintained
placeOrders, batch grouping/processing/formatting, execution parameters,
position/category selectors and recently-placed cache builders, with maintained
tables, helpers and locked Lodash. Only external I/O, mode lookup, caller-ID
validation and the outer fallback decision are scripted. Native assertions cover
requests, errors, warnings, batch sleeps, IDs, amount, coin remaining and both USD
extras. Four native NaN/infinity/-infinity/-0 amount cases are independently
injected on each side; source `parseFloat(qty) || opts.amount || 0` intentionally
caches zero for NaN while sending the source NaN request quantity.

The consumer runner compiles and catches 19 independent faults (17 trait methods,
disposal, constructor admission), then restores exact production bytes and
passes all nine E2Es plus the source-composition test. Its full control run used
the initial 32 finite-case corpus; expanding the same test to 36 cases changed
no production body and passes separately. Both Inverse control runners now bound
every Cargo subprocess to 600 seconds and print private mkdtemp log paths.
Injected TimeoutExpired after a real mutation restores each source byte-for-byte.

Five manifests are reconciled row by row: 52 Inverse receiver rows, five primary
shared-body rows, eight public-socket rows, six private-socket rows and the native
registry entry. Legacy helper mappings remain alongside their concrete consumers;
history/candle/book/recovery class rows retain their pending scope. Parity passes
with 7459 rows and 17198 public Rust items. No new exemption is added.

The reviewed override export changes 1163 entries to 1166: four additions, one
removal and 16 existing body changes, all inside this venue. Every foreign entry
is unchanged. New edges are the dedicated source-super disposal and socket calls,
the Inverse fetch helper reference, and the relocated public connection name
lookup. Existing receiver/account/disposal bodies retain their primary behavior.
The disposal citation independently describes TS BybitInverseExchange.dispose
at lines 260–264, including its actual super.dispose call. All six override
checks, including live source, and all seven cancellation inventory tests,
including discovery export, pass without a new cancellation classification.

Host all-features/all-targets and wasm-library clippy pass for the venue and
registry with warnings denied. Registry equivalence passes for all 48 ordered
source names, its corruption control passes, and all 88 layer checks pass.
The extractor dry run reports zero updates, additions or orphaned symbols.

The mandatory full `venue-oracle:bybit` command passes all 214 tests across 51
test binaries (44 contain tests), with zero failures or ignored tests.
It uses the installed compiler through SAFE_CEX_TYPESCRIPT_PATH.

Current master 789b69c85e merges cleanly. The additive integration preserves all
foreign inventory entries: master 1171 rows become 1174 with the same owned
Bybit delta. All 49 primary/Spot/Inverse receiver tests pass again, including
explicit execution of the two normally ignored source tests. The 18 native
registry tests, seven cancellation and six override checks pass. Final host and
wasm clippy pass with warnings denied. Parity passes at 7459 rows/17204 public
Rust items; the extractor still reports zero changes and all 88 layer checks
pass. No venue production body changed in this master integration.

The subsequent master 7fa2134823 merge preserves #315's per-crate policy shards
byte-identically and adds no Bybit production or inventory change. All three
Bybit native construction tests pass again after the MEXC host integration;
parity and extractor remain green. No removed flat policy file is restored.

## Audit fix: venue-side symbol cancellation (2026-10-17)

Base revision: mirror master `999f0564c2`. Added six production binding tests in
`tests/cancel_all_binding.rs` before changing production code. Running
`cargo test -p safe-cex-venue-bybit --test cancel_all_binding -- --nocapture`
failed all six: four empty-store cases observed zero cancel-all requests;
cancel-all and rejection tests instead reached an unscripted cancel-batch.
After wiring the Exchange/receiver override alone, four empty-store cases passed
and the populated cancel-all case failed with `missing field status`, proving
the partial-row retirement fault. After accepting the source id/symbol shape,
all six pass, including all four variants in the retirement/rejection scenarios.
The code under test is the production constructor, Exchange, retained receiver,
HTTP client and Store; the external wire, clock and scheduler use the binding harness.

Final scoped validation: with the installed compiler selected by
SAFE_CEX_TYPESCRIPT_PATH and runtime dependencies by NODE_PATH,
`cargo test -p safe-cex-venue-bybit -- --include-ignored` passes 223 tests across
47 reported targets, with zero failures and zero ignored tests. Scoped rustfmt
and `git diff --check` pass. A first run without the compiler environment stopped
at the two existing API factory oracles; both pass in the complete run above.

The reviewed override inventory exporter passes and changes 1177 entries to 1178:
one Bybit receiver cancellation edge is added, with the enclosing receiver body
fingerprints refreshed. Every non-Bybit entry remains equal to the base revision.
The inverse manifest now points to cancel_symbol_orders rather than the unrelated
cancel_orders_helper body.

## Zero unbound-name ratchet outside owned buckets

Frozen mirror master `a7ad809e16b28f0d58a3916c068c13a15a6a725e` has13 unbound names:
Bybit2, Bitrue1, Paradex1, Hyperliquid9; all other venue name rows are empty.
MEXC's two old references and stale baseline were already removed by merged649
(`6bfa52bc14`) and659 (`a917a844f1`). Bitrue/Paradex belong to453; HL9 belongs to
the Realm plan. Separate method rows (Bitrue utcOffset; Lighter/WOO pop;
Orderly reverse; Phemex startOf) also belong to453 and are unchanged here.

Bybit's two names have7 exact static sites. `super.dispose` occurs only in
`hyro_private_session::dispose`, with no production caller. Only3 synthetic
oracle cases invoke it. The live route is BaseWebSocket::dispose ->
PrivateProtocol::dispose -> the actual retained Hyro receiver's resetSession,
then core owned-task/timer/socket cleanup. That route and its source export stay;
the orphan wrapper, its test-only base operation and those3 cases are deleted.
The original session corpus is now439 cases, exactly442 minus the3 deleted
wrapper cases, with constructor/reset/ACK/reconnect/disposal-state cases retained.
The obsolete wrapper mutation control is also retired;8 live controls remain.

`unknown callback` occurs in the default arm of6 dispatchers, selected only by
fixed internal lexical callback tags. Native `binding/host::dispatch_callback`
selects the private-read10, snapshot4, publication5, inverse-read3 and account4
names. `binding/session::callback` routes8 socket names; `hyro_socket::callback`
handles current/reconnect itself and selects the6 session-body names. Source
factories allocate precisely these32 body selectors. Closed per-module enums now
encode that already-proved closure; dispatch matches are exhaustive and the6
unreachable intrinsic references are deleted. Nonmatching namespaces retain the
existing host routing; no fake intrinsic registration or scanner exception is
introduced. Live read/publication/account algorithms and socket bodies are kept.

With original master bodies, lowering Bybit's baseline2->0 compiles and fails
`every_venue_reference_has_a_production_binding` for exactly these names:
commit `dfd41f106b`, `/tmp/unbound-bybit-baseline-red.log`. This is a static-dead-code
proof, not a claimed behavioral RED for a path that production cannot reach.
Actual configured Hyro, private refresh and all6 retained source-body witnesses
exercise the preserved production behavior separately. Both old references are
absent from production source after the cutover; owned baselines are unchanged.

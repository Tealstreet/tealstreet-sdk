# KuCoin parity evidence

## Tables and spot token records

Measured baseline: 191 pending rows across 12 manifests. First slice maps 26
rows, leaving 165 pending. Live tables cover 13 futures and 11 spot exports,
including every key/value and duplicate sequence entry.

- `cargo test -p safe-cex-venue-kucoin --jobs 2 --test tables --test records`:
  three tests passed after reviewer correction.
- Rust `RECV_WINDOW` 20000 → 20001: table test failed by `RECV_WINDOW`, exit 101.
- Separately TS `RECV_WINDOW` 20000 → 20001: same named failure, exit 101.
  Both sources restored; test passed afterwards.
- Scoped strict clippy and wasm32-unknown-unknown build passed with jobs 2.
- CI wiring audit: 104 tests passed, including removal and masked-failure
  controls for the mandatory KuCoin step. Uses existing isolated TS compiler.
- Parity invention/status passed: 6906 total rows, 1120 public Rust items.
- Coverage extraction initially failed before execution: missing smol-toml in
  inherited root dependencies. Worktree-local dependency install in progress.
- Adversarial sub-agent review initially rejected fresh-literal record checks.
  Oracle now assigns through inferred variables; three extra-property probes
  changed from false to true, matching structural TS types. Added nested token
  and instanceServers mutations. Independent re-review: MERGE for this slice.
  Review ran in its own nested checkout with no concurrent cargo build.

Merged dependency branches retained 55 named CI steps (core 55, utils 55,
origin integration 52; original branch 54). Added mandatory KuCoin step = 56.

Runtime, API, broker, UTA and socket rows are not covered by this first slice.

## Inert UTA detection

34 ported / 4 divergent / 153 pending after this slice. Four CHOSEN notes
identify the finite JSON boundary: no JS proxies, accessors, symbols, sparse
arrays or custom prototypes. No UTA trading runtime is enabled.

- Both live UTA tests pass; all five tables/records/UTA tests pass together.
- Strict clippy and wasm build passed for the UTA implementation.
- Adversarial sub-agent review: MERGE; safe integers, failure precedence and
  strict union decoders inspected. Added suggested multiple-fault probes.
- Dependency merge of JS primitives retained 56 named CI steps versus 55 in
  its parent. Global parity status now reports shared numeric manifest gaps:
  `round` mapping no longer resolves, and 13 primitive items lack mappings.
  Routed to orchestrator; no venue workaround.
- Coverage dependency now resolves; canonical formatter applied to our three
  manifests after coverage identified serialization-only drift.

## Pure order decisions and mode parsing

Measured 45 ported / 7 divergent / 139 pending. Eleven typed decision helpers
and three CHOSEN JSON-boundary parsers map to explicit free-function targets.
An initial mapping script missed the extractor's `KucoinExchange#` spelling;
the reviewer caught the unchanged rows. The count above is parsed from TOML
after correcting that script, not inferred from implementation count.

- Live decision oracle executes source method initializers for 870 combinations.
- Live parser oracle covers 352 cases, including Unicode casing, arrays, own
  non-callable toString, absent values and strict crossMode false semantics.
- All seven crate tests pass; scoped strict clippy and wasm build pass.
- Wrong reduce-only close side substituted in Rust: exit 101, named
  `live_order_decisions`; restored source passed the full scoped suite.
- Adversarial sub-agent review: MERGE after correcting mapping/count and
  documenting three parser domain restrictions as CHOSEN.
- Coverage extraction passes with zero changes/additions/orphans.

## Broker attribution and reference signing capture

Measured 47 ported / 10 divergent / 134 pending. Three broker header rows
explicitly choose the core normalized string-header boundary; full Axios header
value/name normalization remains a host responsibility.

- Live broker oracle: 137 combinations passed, including verify false, existing
  lowercase headers, empty key/config, BOM trimming and NEL preservation.
- Fourteen API reference captures execute live TS interceptors and actual Axios
  transformRequest/getUri. Futures signing query is unescaped; spot is escaped.
  Valid JSON strings sign quoted and transmit unquoted; non-JSON text is quoted
  on both boundaries. Missing optional members are omitted from object JSON.
  This is reference capture only, not a Rust API parity claim.
- Both tests and scoped strict clippy passed. Independent review: MERGE.
- Core REST/fallible-handler and JSON primitive merges retained all 56 CI steps.
  A merge-resolution double projection in two transport tests was corrected;
  targeted heartbeat timeout test passes (65 unrelated tests filtered out).

## Sizing and wire order mapping

Measured 56 ported / 12 divergent / 123 pending. Nine pure sizing/symbol
helpers preserve production Store behavior; two mapper rows explicitly record
MIXED typed-enum prototype correction and JSON/string-identity restrictions.

- Sizing live oracle passes 255 fixtures, including duplicate stored ids,
  smaller/stale/zero/missing native remainders, replacement ids and ticker priority.
- Futures/spot mapper oracle passes numeric coercion, stops, attached TP/SL,
  time slicing, prototype keys and malformed identity witnesses.
- Initial sub-agent review rejected numeric comparison: JSON collapsed all
  nonfinite classes to null. Both sides now tag NaN and signed infinities.
- Negative control applying abs to parsed numbers fails exit 101 in
  live_wire_order_mapping, explicitly Infinity versus -Infinity. Restored tests pass.
- CI wiring audit passes all 104 tests with both new oracle binaries pinned.
- Independent re-review: MERGE after executing tagged Node probes for both venues.
- Scoped strict clippy and wasm32 build passed after the correction.

## Integration and isolated oracle dependencies

Integration merge f7ad8e7ba5 retained both parents' CI step multisets (56/55 ->56),
all crate dependencies and shared exemption registrations. Scoped compilation,
layer audit, global parity (6906 rows/1203 public items), extraction (zero drift),
13 live venue tests and 104 wiring tests passed. Sub-agent merge review: MERGE.

The isolated CI selector set appends axios ^1.17.0, qs ^6.16.0 and create-hmac
^1.1.7 from the owning package, with the copied root lockfile and frozen install.
Both broker and API capture use real create-hmac and resolve dependencies beside
SAFE_CEX_TYPESCRIPT_PATH. The named venue step runs after the root install, but
the native Cargo test matrix also discovers these tests and requires isolation.

A disposable tracked-source mirror under /private/tmp, with no ancestor
node_modules, matched all 14 API captures. Missing-axios import mutation failed
MODULE_NOT_FOUND; in-memory TS RECV_WINDOW mutation changed 20000 to20001. Mirror
was deleted. Mandatory Cargo child controls require the source mutation to fail
by name at101 and missing dependency to fail loudly. Isolated tests and clippy pass.
Independent sub-agent isolation review: MERGE; real dependency and mutation probes pass.

## Pure API helpers

Measured57 ported /13 divergent /121 pending. Query serializer uses shared
JsJsonValue and JS own-key/number spelling, preserving missing values and BigInt;
CHOSEN note records non-JSON object and UTF-16 exclusions. Spot public endpoint
matching retains literal prefix behavior, including longer suffixed paths.

Live extracted helper + real qs test passes; inverted encoding flag fails101 by
name, restored test passes. Isolated tracked-only mirror query probe passes and
mirror is deleted. Scoped clippy/wasm pass. Sub-agent review: MERGE.

## Futures payload formatters

Measured59 ported /13 divergent /119 pending. Normal formatting calls shared
BaseExchange client-ID preflight; algo formatting intentionally generates a UUID.
Store values are copied before runtime/event hooks to avoid borrowed state
across reentrant callbacks. No local core validation or numeric shim is added.

Live TS tests execute both formatters and actual base ID validation methods,
comparing wire bytes, own keys (including undefined), errors and UUID call counts.
Full-position/partial stops, post-only GTC, attached stops, negative half ties,
nonfinite inputs, missing markets, invalid IDs and oversized native orders pass.
Changing closeOrder true to false fails101 by name; restored source passes.
Scoped strict clippy, wasm build and layer audit pass. Isolated tracked-only
Node mirror passes and is deleted. Sub-agent review: MERGE, with algo manifest
note corrected to describe UUID generation without claiming client-ID validation.

## Retry reference boundary (factories remain pending)

Real Axios and axios-retry execute14 fixtures with deterministic elapsed time.
Both venues re-sign later attempts with a fresh timestamp over the serialized
string body, so the signed preimage equals the unchanged wire bytes. Reads and bullet POSTs retry; order writes and response-bearing
failures do not. The default remaining timeout shrinks:8000ms per attempt gives
20000/12000/4000 and3 attempts;21000ms on the first gives one attempt.

This shared timeout-budget/RestBody seam was routed to core, with the factory
rows held. No venue retry loop or encoded-body shim was added. axios-retry
^4.5.0 was appended to isolated selectors from the owning package. All14 fixtures
pass in a tracked-only no-ancestor-node_modules mirror, deleted afterwards.
Missing-retry import fails MODULE_NOT_FOUND; source retries3->0 mutation is a
mandatory child test that fails101 by name. Scoped tests/clippy pass.
Independent sub-agent reference-capture review: MERGE. Counts unchanged59/13/119.

## Websocket pure helpers

Measured62 ported /16 divergent /113 pending. Live-source checks cover the
public role union, three numeric/timestamp parsers, filtered market sharding
and candle topic splitting. Numeric comparisons preserve NaN, both infinities
and negative zero. JSON-domain restrictions are explicit CHOSEN notes.

Threshold > to >= mutation fails101 in live_socket_number_and_timestamp_helpers;
restored tests pass. Strict scoped clippy and wasm build pass. Tracked-source
mirror with no ancestor node_modules passes and is deleted. Independent
sub-agent review: MERGE. Runtime connect/token preparation remains pending.

## Spot store-only helpers

Measured67 ported /16 divergent /108 pending across191 rows. Five methods use
the production Store: synthetic positions, max leverage, leverage update and
the two intentional mode no-ops. Live source checks distinguish omitted/empty
selection, duplicate markets, inactive markets and whole leverage-row replacement;
subscription count verifies exactly one update. Rust and TypeScript buy-leverage
1->2 controls each fail101 by name; restored tests pass. Scoped strict clippy
and wasm pass, no-ancestor-module tracked-source mirror passes and is deleted.

Sub-agent review caught an inherited constructor property in the one-off manifest
mapping script: the constructor row was incorrectly mapped to undefined. Restored
it to pending and re-review returned MERGE. Only five intended rows change status.
Global parity passes6906 rows/1218 items; wiring104/104 and extraction0 changes.

## REST account readers

Measured67 ported /18 divergent /106 pending. Both reads use production
BaseExchange error reporting and response-bearing RestAdapter directly; no
factory, signing or retry behavior is claimed.28 live cases pin endpoints,
strict success codes, spot UID fallback, missing-data errors and rejection-body
precedence. Reversing body/message precedence independently in Rust and TS
fails101 by name.18 live non-string-ID witnesses pin the CHOSEN typed-account
fallback; dropping an unpolled/pending read leaves no latch or late error.

Double-escaped lone-surrogate witnesses preserve valid TypeScript JSON acceptance
and explicit Rust representation refusal, including the failure-response path;
scalar Unicode, astral pairs, BOM and space remain accepted. No invalid-JSON
claim or replacement character is used for these refusals. The shared canonical
UTF8 declaration is cited alongside local typed-account restrictions.

Five scoped tests, strict clippy and wasm pass. Isolated tracked-source oracle
passes with no ancestor node_modules and mirror deleted. Layer audit37 passes,
wiring114/114 and global parity6906 rows/1231 items. Sub-agent account review
and follow-up representation-refusal review both MERGE.

## Protocol-table lookup census

TypeScript AST census:11 element accesses of named constant protocol tables.
- Futures mapOrder: ORDER_STATUS, ORDER_TYPE, TIME_IN_FORCE: each result
  is the left operand of || and inherited functions suppress its fallback.
- Spot mapOrder: KUCOIN_SPOT_ORDER_STATUS, KUCOIN_SPOT_ORDER_TYPE,
  KUCOIN_SPOT_TIME_IN_FORCE: same three control-flow effects.
- Spot mapOrder additionally indexes ORDER_STATUS with active ? active : done
  as the RHS fallback; those two literal keys cannot reach inherited members.
- Futures fetchOHLCV INTERVAL, public listenOHLCV WS_INTERVAL and the two
  spot fetch/listen KUCOIN_SPOT_INTERVAL lookups have no result truthiness test
  or || fallback. Their keys are caller-provided interval options.

The two mapper rows previously described a chosen typed-enum correction. The
preserve-first ruling makes that an unresolved FINDING, now stated explicitly.
All eleven inherited-function names at each of six wire-controlled lookups have
live witnesses proving the default is suppressed. No local prototype map or
shared enum widening is introduced. Existing __proto__ object witness remains.

Store dictionary follow-on:24 live controls cover twelve inherited names with
amount0/1. Create consumes0 IDs on both sides; its mismatch is the guard/error
message, not an ID-count difference. Algo consumes1 in TS versus0 in Rust;
amount1 fails after that allocation, while amount0/limit succeeds with closeOrder
and omitted wire symbol. Both formatter rows now say FINDING. Counts are
65 ported /20 divergent /106 pending, correcting earlier ported claims.

The broader bracket census includes17 marketsBySymbol accesses,1 marketsById,
5 tickersBySymbol,26 messageHandlers accesses (reads/writes/deletes) and2 balance
currency/coin writes. The three getUsdPrice keys append closed -USDT/-USDC/-USD
suffixes, and handler keys use candle_/orderBook_ namespaces: those cannot spell
inherited names. The spot status fallback's active/done keys are closed source
literals rather than wire keys. No uppercasing or other immunity was added.

Final inherited-member census expands to all12 stock names:66 mapper function
witnesses and24 formatter controls pass. Mapper oracle records typeof=function
BEFORE JSON serialization, alongside field omission afterward. Independent
sub-agent finding review and follow-up pre-serialization review: MERGE. Scoped
mapper/formatter tests, strict clippy and global parity pass.

Live Spot balance write probe: constructor/toString become ordinary own keys;
__proto__ instead changes both dictionaries' prototypes (currency prototype.free
and coin prototype.base retain the assigned values), with no own __proto__ key
and empty currencies/coins in wire JSON. fetchBalance stays pending for the
shared write operation; no local prototype setter is implemented.

### Maintained balance dictionary write witness

`balance_reference` executes live Spot `fetchBalance` and `getUsdPrice` for an
ordinary currency and all twelve Object.prototype names. Eleven callable names
become ordinary own entries and survive JSON serialization. `__proto__` replaces
only the currencies/coins dictionary prototype, creates no own key, and emits
`{}` for those dictionaries while `info` retains the account row. Negative checks
pin that Object.prototype and fresh unrelated objects acquire neither free nor
base. A separate, explicitly counterfactual nested-property write demonstrates
the wider global effect, with the scratch field removed in finally.

The maintained child control substitutes Object.create(null) for the source
currencies dictionary and requires the named live Rust witness to fail at 101.
This is reference evidence only: Spot fetchBalance stays pending for the shared
write operation; no venue-local prototype setter or production Rust mapping is
introduced by this test.

### Shared inherited-member lookup adoption

Both order mappers now call shared `js_table_lookup` and its truthiness predicate;
all mapped enum strings are nonempty. Inherited hits suppress the source fallback.
Futures stop overrides run before typed publication, so an inherited type that is
subsequently replaced can still return an ordinary Order. Spot postOnly bypasses
the TIF lookup exactly as the source does. Remaining inherited outputs refuse the
whole record at the shared closed-enum boundary, rather than emitting invented
fallback values. This is CHOSEN for the eleven callables under the canonical
closed-typed-output-boundary declaration; __proto__ remains a separate FINDING.

The corpus contains 66 pre-JSON callable witnesses, six object witnesses asserting
that __proto__ remains `{}` in serialized fields, and eight source override cases.
TypeScript retains records where Rust refuses; no serialized convergence is
claimed. A compiling Rust mutation erasing Inherited to Missing failed the named
callable test at 101. A maintained child control changes the source status table
to a null-prototype literal and requires 101 plus the pre-JSON typeof assertion.

Follow-up review found that typed publication initially preceded attached-order
coercion. It now follows all SL/TP processing. Four extra live cases pin that an
uncoercible attached price wins over parent refusal, and that a valid attached
child exists in the source result even when Result<Vec<Order>> refuses the whole
Rust batch. The futures manifest explicitly records this child loss.


### Formatter standard prototype and remaining Store write boundary

120 live cases cover all twelve inherited names, zero/nonzero amounts and five
order types. Shared TableHit now retains the missing-market guard decision;
create reads price precision before consuming any UUID, while algo consumes one
before amount precision. Zero-amount limit/market algo formatting retains the
undefined own symbol key and emits the source's symbol-less closeOrder payload.

The oracle now executes the source DefaultStore constructor, replacing an
Object.fromEntries fixture that hid prototype replacement. A market whose symbol
is __proto__ changes the dictionary prototype; subsequently looking up precision
finds the market's precision object. The zero-amount algo path again succeeds
with no wire symbol and one UUID, while Rust's typed own-entry Store refuses
before allocating one. Create consumes zero in both but has a different guard
error. Both rows remain FINDING for this shared write boundary. A maintained
control changes the source assignment to defineProperty and must fail at 101
with the specific inherited-precision assertion; no setter is implemented here.
The formatter oracle also passed in a tracked-source mirror without ancestor
node_modules using the isolated compiler/runtime dependencies. A compiling Rust
mutation treating Inherited as Missing failed the named 120-case formatter test
at 101; the implementation was restored afterward.

### Public subscription batching

`send_channel_op` executes the live source method over 356 cases, comparing
serialized frame bytes, channel/chunk ordering, Date.now call counts and first
send-error short-circuiting. Boundaries include 99/100/101 and 199/200/201 symbols,
empty channels, repeated channels, empty symbols, commas, quotes, newline, BOM
and astral text. One empty symbol skips its chunk; two join to a comma and send.
The shared JSON encoder retains object field insertion order.

A maintained source control changes the slice end to 99 and fails the exact
wire-order assertion at exit 101. A separately compiled Rust chunks(99) mutation
fails that assertion at 101; the source was restored. The double-escaped lone
surrogate witness records valid JavaScript acceptance and exact escaped output,
while the shared UTF-8 ingress refuses it. The row cites the canonical CHOSEN
representation declaration, rather than claiming unrestricted string parity.

The new oracle passed from a disposable tracked-source mirror with no ancestor
node_modules, resolving the isolated compiler. Selecting a nonexistent compiler
failed MODULE_NOT_FOUND. The mirror was deleted. No runtime npm dependency was
added. The existing mandatory ws_helpers target discovers these tests.

Validation: five ws_helpers tests passed after restoring the Rust mutation;
sub-agent review returned MERGE after independent live Node probes. Host strict
Clippy with --locked --all-targets --jobs 2 passed. The separately run wasm32
strict Clippy stopped before KuCoin in the shared candle-cache dependency:
indexed_db.rs lines 202 and 207, collapsible_if. This is not a wasm lint pass.
Global parity passed at 6,984 rows / 1,301 public Rust items. Own measured counts:
65 ported, 21 divergent, 105 pending across 191 rows.

### Futures order-book reads and category preservation

The live oracle executes fetchNormalOrders, fetchAlgoOrders, fetchOrders and
mapOrder, plus existing source sizing/reconciliation methods. It initializes
market dictionaries with the actual Store constructor. Forty-eight matching
cases cover exact GET paths/active params/order, strict string response code,
missing/nonarray items, error body precedence, failed versus empty books,
late store updates during the second request, same-id native remainders and
separately tagged NaN/positive/negative infinity before serialization.

108 inherited-value cases retain typeof before JSON for all twelve prototype
names across status/type/timeInForce and all three readers. The source keeps
three fresh records including valid siblings; Rust refuses the whole book.
Combined reads consequently preserve the old normal category. Eleven callable
fields disappear on JSON serialization; __proto__ stays present as {}. Neither
is described as serialized convergence. Nine additional witnesses cover lone
surrogate IDs, numeric error messages and null rows (source fails while reading
updateTime; Rust refuses an object boundary). All three rows record these causes.

Dropping a pending first or second REST read releases the adapter future, sends
no subsequent request, leaves Store unchanged and permits a fresh call. The
maintained source failure-as-empty mutation fails the named preservation
assertion at 101. A compiling Rust mutation returning Some(empty) on failure
fails the same assertion at 101 and was restored. Host strict Clippy passes.
The tracked-source mirror oracle passed without ancestor node_modules; an
absent isolated compiler failed MODULE_NOT_FOUND. The mirror was deleted.
The mandatory CI oracle and its wiring assertion now include order_reads.

The reader requires the shared VirtualClock explicitly. A browser-profile clock
with a 5-second server offset distinguishes it from BaseExchange's wall clock;
source timestamped rows make zero fallback-clock reads. A compiling wall-clock
substitution fails the named live assertion at 101. Host strict Clippy and all
five order-read tests passed with the explicit shared-clock boundary.

The published owned-snapshot-vs-live-alias declaration is now merged and cited.
Three live alias witnesses assert source Order.info identity with the response
row, then mutate its id and price. Source info observes the relabel and NaN;
mapped order.price retains its earlier numeric value. Rust info retains its
original string values and numeric order.price also remains unchanged. This
witness distinguishes aliasing from a claim about non-finite ingress encoding.

Final validation after integration merge 8b625d1907: six scoped order-read tests,
host strict Clippy and explicit wasm32 strict Clippy (--locked --jobs 2
--all-targets) pass. CI wiring passes 193/193, global parity passes 6,984 rows /
1,365 public Rust items, and the clock/alias re-review returned MERGE. Measured
KuCoin inventory: 65 ported / 24 divergent / 102 pending across 191 rows.

### Consolidation against integration 91db928805

The numeric Store ABI and Axios metadata merges retain the exact CI step
multiset unions: 76/76 to 77, then 77/76 to 77. Real tomllib parsing found
299/299 then 301/301 exemption blocks/distinct targets, with every parent
reason retained and no blank reason. The own Cargo.toml dependencies are
byte-identical to the pre-merge file. Layer audit classifies 37 crates.

All 46 KuCoin tests pass with zero ignored, using the isolated TypeScript
installation. Host strict Clippy and wasm32-unknown-unknown strict Clippy
were run separately with --all-targets, --locked and --jobs 2; both pass.
Wiring passes 195 checks; extractor reports zero added/orphaned/drift; global
parity passes 6984 rows and 1515 public items. Measured venue status remains
65 ported / 24 divergent / 102 pending across 191 rows. Factory WIP is not
part of this consolidation and makes no manifest claim.

### Spot active-symbol discovery

The source fetchActiveOrderSymbols is executed directly against 29 JSON cases:
strict code checks, wrong shapes, failures, mixed array entries and overflow.
The Rust helper returns prepared JSON entries without string filtering. Separate
source/owned-array witnesses pin later gateway mutation, and a double-escaped
lone-surrogate witness pins valid JavaScript JSON acceptance versus the Rust
representation refusal and empty-array fallback. Ordinary Unicode, astral pairs
and whitespace remain in the positive corpus.

Seven numeric entries are compared as f64 bits before JSON: -0/-0.0/-0e300 each
retain 8000000000000000 and both overflow signs retain their infinity bits.
This helper returns Value; no typed f64 Deserialize was changed or rebuffered.
Cancellation tests drop the adapter wait twice and verify immediate release and
re-entry without a latch. A compiled Rust string-filter substitution and a
separate live-source String coercion both fail the named array assertion at101.

Six scoped tests pass, host and wasm strict Clippy pass separately. The oracle
runs in a tracked-source mirror without ancestor node_modules; ordinary,
overflow, alias and surrogate probes pass and absent compiler fails loudly.
The mirror is deleted. Wiring195, layer37 and zero extractor drift pass. The
private-to-free-function rename is explicit in the manifest note. Measured
status becomes65ported/25divergent/101pending across191; factories remain held.

### Existing numeric ingress audit

Four declared numeric fields span three mapped type rows and four Rust public
projections: spot WS server, token data/response, and tagged account-mode
detection. The maintained oracle compiles 64 assignments against actual source
declarations, then captures JSON.parse bits before serialization. Raw, owned
Value, borrowed Value and native negative-zero controls yield 200 checks:
160 matching controls, 36 refusals and four lexical -0 sign losses.

Spot raw typed overflow refuses +/-1e400 while Value paths retain it. The tagged
account-mode decoder loses lexical Value -0 in both arrays and also refuses
valid decimal/exponent spellings on the recorded raw/Value paths. These are
existing defects, not a Value-first candidate. All three rows are FINDINGs
awaiting shared ingress; no local decoder workaround was introduced.

A compiling Rust +0 normalization mutation fails the named positive-bit boundary
at 101; changing the live pingInterval type to string separately fails the live
compiler assertion at 101. The tracked-source mirror runs all 64 cases without
ancestor node_modules; missing compiler fails MODULE_NOT_FOUND. Mirror deleted.
Scoped records/account-mode/numeric tests pass. Host and wasm strict Clippy pass
separately; wiring195, layer37 and global parity6984/1516 pass. Measured status
is62ported/28divergent/101pending across191 rows.

### Integration b8fc4dfbaa merge-down

Both-parent Rust CI step multisets77/82 merge to82 with zero missing entries.
The additive package.json conflict keeps Asterdex's gate and KuCoin's newest
spot-read/numeric audit gates. Wiring217 passes. Own crate dependencies are
unchanged and immediate locked compilation passes. Layer audit37 passes.
Real tomllib parse reports312 blocks and312 distinct exemption targets, zero
blank reasons. All parent targets survive; js_number_string's revised reason
is the incoming shared-owner wording, rather than a lost block.

All54 venue tests pass with zero ignored. Host and wasm strict Clippy pass
separately with all-targets/locked/jobs2. Extractor reports zero added/orphaned
and zero drift. The merge retains62ported/28divergent/101pending.

### Spot market read

The actual source method and production Store constructor/getter execute over
82 positive/error response cases. Four mapped numeric fields are compared as
f64 bits before JSON, including signed zero and both overflow signs; native
NaN/infinities are tagged distinctly in serialized comparisons. Transport errors
preserve the response-message preference; fallback observes the store after the
await. Dropping the adapter wait twice releases it without a latch.

Separate witnesses pin gateway-to-info and store-to-returned-array live aliases
against owned Rust snapshots. Twelve missing/null/non-string open-field cases
produce source records but Rust refuses the mapping and returns the store; a
truthy numeric error payload is also an explicit shared string-input finding.
A double-escaped lone-surrogate witness records source acceptance and Rust
representation refusal; scalar Unicode, astral pairs, whitespace and inherited
name strings remain positive cases. No closed-enum declaration is used for
these open-string findings.

A Rust abs mutation compiles then fails the named market assertion at101; a
separate live-source parseFloat-to-Number mutation fails it at101. A disposable
tracked-source mirror without ancestor node_modules retains the signed-zero
bits and fails MODULE_NOT_FOUND for a missing compiler; it was deleted.

The local extractor resolves smol-toml from this worktree's node_modules. Its
zero-drift result is a local-environment check, not proof of clean-install wiring;
the structural dependency defect described in RESUME.md belongs to the gate
owner. Measured venue counts become62ported/29divergent/100pending across191.

Adversarial review found a swallowed failure-body decode refusal: .ok()
discarded valid lone-surrogate JSON and selected the transport message. The
fixed path retains the decode error and reports representation refusal. A raw
failure-body witness pins TypeScript's surrogate rejection text separately from
Rust's diagnostic, with ordinary Unicode/astral/whitespace messages positive.

Re-review returned MERGE after independent Node probes. All13 scoped Spot tests
pass; host strict Clippy and wasm strict Clippy pass separately. Wiring217,
layer37 and parity7334/2172 pass. A second compiling Rust mutation that swallowed
the failure-body refusal fails the new named witness at101 and was restored.

### Integration 56e818d52b merge-down

The merge is textually clean. Both-parent Rust CI multisets82/86 merge to86
with zero missing steps. Own Cargo dependencies are byte-identical; immediate
locked compilation passes. Real tomllib parse reports312 blocks/312 distinct
exemptions, all parent reasons retained, zero blank reasons. Wiring230 and
layer37 pass. All61 own-crate tests pass with zero ignored; host strict Clippy
and wasm strict Clippy pass separately. Global parity passes7334rows/2507items.
Extractor reports zero local drift with the previously stated clean-install
caveat. Counts remain62ported/29divergent/100pending.

Merged-state reviewer returned MERGE: exact parent CI union, exemption records
and reasons, lockfile, selectors, dependency list, root script additions and
worker layer containers verified independently.

Before readiness, integration advanced to7033cad93b with Lighter records and a
shared ingress-spec correction. Re-merge keeps86/86->86 CI steps and312/312
exemptions with identical reasons/dependencies. All61 scoped tests and both
strict Clippy targets pass again; parity7334/2513, wiring230, layer37 and local
extractor zero drift pass. The clarification agrees with this venue's separate
raw/owned/borrowed numeric-bit witnesses; no type finding was closed by it.

A second head check found Binance's consolidation at e7c0d8c4d2. The follow-up
changes only Binance files: lock, layer policy, KuCoin dependencies/selectors,
root package and wiring are byte-identical to the reviewed parent. CI remains
86/86->86; exemptions312/312 retain every reason. All61 venue tests, host and
separate wasm strict Clippy, parity7334/2516, wiring230, layer37 and the local
extractor drift check pass on this tree.

The docs-only d11a7615df follow-up was independently reviewed MERGE; no executable
file changed. Integration then advanced to c659b17b74 with shared numeric ingress
and GRVT updates. Exact CI86/87->87; real TOML323 blocks/323 distinct, every
parent record/reason retained. Own dependencies, layer policy and selectors
unchanged; immediate locked compile passes. All61 own tests, host and separate
wasm strict Clippy, parity7334/2531, wiring237 and layer38 pass. Extractor zero
drift is still local-only evidence with the smol-toml clean-install caveat.
Merged-state reviewer returned MERGE, including the lock's dependency-edge-only
change. No numeric row has been closed just because the shared operation landed.

### Shared numeric ingress adoption

Exhaustive JsNumberOutput expression registrations now cover the three Spot
token projections plus account-mode enums. There is no shared-type orphan wall.
All72 actual-source numeric cases pass224 raw/owned/borrowed/native bit checks,
including negative underflow; the legacy200 checks still expose36 refusals and
4 sign losses, with160 positives. No production callers decode these records
yet. The three manifest rows remain FINDINGs for bare serde entry points; this
is supported-entry repair, not a claim that registration changes Deserialize.

The account-mode serializer's intermediate json! Value erased infinities before
the output guard. It now emits native fields directly. Restoring the old buffer
COMPILED then failed101 at the named classicSubAccount[0] loss; an abs mutation
in the server decoder COMPILED then failed101 at pingInterval bits. Both were
restored. The live source number-to-string mutation also fails101 by name.

A disposable tracked-source mirror ran72 cases with no ancestor node_modules;
missing compiler failed MODULE_NOT_FOUND, and the mirror was deleted. Seven
scoped tests, host strict Clippy, separate wasm strict Clippy and parity7334/2531
pass. Adversarial reviewer returned MERGE after independent Node72-case replay
and exhaustive field/buffer/claim audit. Counts stay62/29/100 across191.

### Capture inside public numeric Deserialize

The registration-only limit above is real but not the end of the repair. After
reading Binance9d7f0ef7d7, each of our four public decoder projections now captures
JsNumberInput before the private map/tagged Wire decoder and runs decode_with.
The old refusal assertion flipped: raw1e400 yielded native positive Infinity.
The replacement corpus passes520 comparisons across raw str/slice, owned/borrowed
Value, all three shared entry APIs and native-0 controls (72 live source cases).
Bypassing the capture separately in Spot and account-mode production decoders
COMPILED then failed101 at the named direct-ingress assertion. Both restored.

A further30 actual-source string cases retain ten lone-surrogate acceptances
alongside20 scalar/astral/whitespace/protocol-name positives. Rust's two entry
paths refuse the ten representation cases without replacement characters. Thus
the two Spot type rows remain CHOSEN under the shared UTF-8 declaration; only
account-mode detection moves to ported. Candidate counts63/28/100 across191.
The tracked-source mirror ran72+30 cases with no ancestor node_modules and a
MODULE_NOT_FOUND missing-compiler control, then was deleted.

Final capture validation:63 own-crate tests, zero ignored; host strict Clippy and
separate wasm strict Clippy pass. Global parity7334/2531 passes; extractor zero
local drift retains the clean-install caveat. Re-review returned MERGE after
independent72numeric/30string source replay and capture/shape/output audit.
Counts measured63ported/28divergent/100pending.

### Integration88727297a0 merge-down

Exact CI87/87->87, real TOML327 blocks/327 distinct retaining every parent
record/reason. KuCoin dependencies, lockfile, selectors and layer policy remain
byte-identical. Immediate scoped compile and63 own tests pass; host and separate
wasm strict Clippy pass. Parity7334/2536, wiring237, layer38 and local extractor
zero drift pass. The smol-toml clean-install caveat remains. Merged-state reviewer
returned MERGE, checking opt-in eligibility additions against our public capture
and output checks. Counts63/28/100.

## Store notification sweep

Audited all implemented KuCoin Rust store accesses: one writer, Spot
`setLeverage` via `Store::update`, and zero `set_setting` callers. Reclassified
that formerly ported row to FINDING on the routed shared notification seam.
The actual TypeScript DefaultStore constructor/update/subscribe path confirms
state commits before a subscriber throws, the second listener is skipped, and
the async venue method rejects with the identical error. Four live cases cover
ordinary and empty symbols, with and without the subscriber failure. Rust's
supported infallible path commits state and calls both listeners; no panic is
introduced as a substitute for a recoverable error.

`cargo test --locked -p safe-cex-venue-kucoin --jobs 2 --test spot_helpers`:
3 passed, zero ignored. The maintained catch-and-continue source mutation
compiles and its child fails 101 at `subscriber throw skips the second listener`.
Scoped host strict clippy on this test passes. The actual source oracle also
passes in a disposable tracked-source mirror with no ancestor node_modules;
changing its isolated lodash import fails MODULE_NOT_FOUND. Mirror removed.
This slice changes no wasm implementation; prior separate wasm lint evidence
above is not being reported as a fresh run. Counts: 62 ported / 29 divergent /
100 pending across 191 rows. Extractor evidence remains local only.

### Notification reachability follow-up

[Reachability audit](STORE_NOTIFICATION_AUDIT.md): 30 remaining ported callables
resolved via TypeScript AST and 32 data/types, no additional ported notification
path. One already-divergent socket send helper gains an independent FINDING.
The actual source send/reconnect/close/health chain with production DefaultStore
pins its unawaited rejection separately from Spot setLeverage's own rejection.
Five spot_helpers tests pass, zero ignored; host strict clippy on that test passes.
The new catch-and-continue child fails 101 at `channel send notification skips
later subscriber`. Two socket-path cases pass in a disposable tracked-source
mirror with no ancestor node_modules; removed afterward. Counts remain62/29/100.

Merge-down `469cb47e90` from aa632b5578 passed65 own-crate tests, host and separate
wasm strict clippy, parity7334/2536, wiring237, layers38 and local extractor zero
drift (local smol-toml, not clean-install proof). Exact CI union87/87->87;
real tomllib327 blocks/327 distinct, both-parent records/reasons retained;
KuCoin Cargo.toml and lock unchanged. Merged-state reviewer returned MERGE and
independently verified the30-callable reachability census.

## API factory composition after core remote staging

The held remote-input FINDING assertion failed101 after merge469cb47e90 because
Rust now sends the source transformed primitive to the signer. Converted to a
positive18-case comparison of signer configs and echo inner-adapter values;
four object values remain objects BEFORE byte projection. The test adapter no
longer projects every value through wire_bytes, which would hide that distinction.
Both factories use shared execute_with_interceptor rather than pre-signing a
request before ordinary execute. Local14 retry/signature/budget cases and48
runtime cases cover body/header/public-prefix/form and remote behavior. The
signature-rejection resend re-signs once; `signature_predicate_matches_actual_typescript`
pins the predicate against live source.

20 boundary cases distinguish the actual source directions of options aliasing:
futures captures key/authentication defaults while Spot sees later key/privacy
changes; both see later nested broker config. Other cases witness false header
suppression (not the string false), source lone-surrogate acceptance/Rust ingress
refusal, and positive scalar/astral/whitespace inputs. These are CHOSEN rows with
shared declarations and the normalized header rationale, not repaired findings.
The source factory's safeCexAxiosCreate/transport-install seam is injected; these
checks do not claim production routing, authorization or telemetry acceptance.

Cancellation drops both factories at remote signing, initial transport and the
transport attempt after a retry, twice per phase. Resources release and the next
request can enter; the signer-pending path transmits nothing. Default retry delay
is zero, so there is no venue retry-sleep future. No local task or latch exists.

Controls: changing the Rust signing query encode flag to the opposite value
COMPILES and fails101 at `live KuCoin factory wire/signature/deadline`; restored.
Maintained source retry/public-branch controls fail named assertions in child101.
Missing runtime dependencies fail loudly, including nanoid.

Only new isolated selector: nanoid ~6.0.1, read from safe-cex/package.json.
Generated the full isolated manifest through ci-deps.cjs and installed against a
copied root lock with --frozen-lockfile --ignore-scripts --non-interactive. A
tracked-source /private/tmp mirror with no ancestor node_modules passed48 runtime,
20 boundary and14 retry cases using that install. Missing nanoid and missing
compiler controls both failed MODULE_NOT_FOUND. Mirror removed.

Scoped full venue suite75 passed, zero ignored; host strict clippy and separate
wasm strict clippy passed. Parity7334 rows/2538 public items, wiring237, layers38,
local extractor0 drift (local smol-toml, not clean-install wiring proof).
Counts62 ported/31 divergent/98 pending across191. Final adversarial reviewer
returned MERGE and independently reran48 runtime/20 boundary/14 retry cases.

## Spot account validation

The live method oracle compares 24 account rejection, transport error and balance
cases, including native numeric bits before serialization. Six tests also pin
non-string error values, own undefined versus missing balance, both views of a
live balance alias, raw failure-body lone surrogates with scalar/astral/whitespace
positives, and dropping each of the two await points twice. The async balance
callback is a dependency fixture, not an implementation of the held balance row.

The source mutation removes the balance failure catch and fails a child101 at
`live Spot validation ordering`. A plausible Rust mutation propagating balance
failure as validation failure COMPILES and fails the same named assertion on the
balanceFailure case; restored. A simpler omitted-fetch mutation also failed but
is not the evidence for failure disposition. JSON comparisons use the shared JS
number spelling projection; the independent bits field prevents signed-zero and
non-finite differences being erased by that projection.

A tracked-source /private/tmp mirror passed three success/failure/surrogate
cases with no node_modules ancestor using the fresh isolated compiler. Missing
compiler failed MODULE_NOT_FOUND. The first copy lacked tracked logging source
and failed loudly; the corrected copy included that source. Mirrors removed.
After review the oracle uses installed Axios through the isolated compiler
resolver; Axios is already in the isolated selectors, so no new selector is
needed. Mandatory venue-oracle:kucoin and
its exact wiring assertion now include spot_validation.

The row retains a routed FINDING for truthy numeric/object errors that the shared
ValidateAccountResult.error String cannot carry, separately from CHOSEN UTF-8,
owned balance snapshot and own-undefined presence differences. It does not cite
the closed-enum boundary for an open string field.

Before slice review: full scoped suite81 passed, zero ignored; host strict
clippy and separate wasm strict clippy passed after replacing a test-only
needless index loop. The six slice tests passed again after that lint fix.
Parity7334/2560, wiring241/241, local extractor zero drift (locally resolved
smol-toml, not clean-install proof). Candidate62/32/97 pending review.

The first review returned one blocker: unconditional JSON.parse in the raw-body
fixture hid Axios preserving empty/non-JSON bodies as strings. The oracle now
executes actual Axios default transformResponse, and a named witness records
source fallback without emission versus Rust whole-body diagnostic plus emission.
This is a separate shared response-staging FINDING, not UTF-8. A fresh tracked
mirror passes five cases including both raw-body cases; missing Axios and missing
compiler each fail MODULE_NOT_FOUND. Both controls are also maintained in Rust.

Re-review returned MERGE after independently executing the actual Axios raw-body
probes. Eight slice tests and host/separate wasm strict clippy pass on the repaired
oracle. Final measured slice count62 ported/32 divergent/97 pending across191.

## Spot ticker reads (candidate, review pending)

108 actual-source request/numeric/fallback cases compare native f64 bits before
JSON, including lexical -0, overflow, parseFloat suffixes and missing/falsy
values. Store fallback is read after the await. The oracle uses the production
DefaultStore update and installed Axios response transform, never fromEntries
or unconditional JSON.parse of response bytes.

All twelve inherited market keys pass the source guard and produce a retained
ticker whose id/symbol are own undefined values, absent on JSON. Shared TableHit
preserves the guard; Rust reports a typed-output refusal and returns current
Store tickers. This open-string undefined finding is not the closed-enum
boundary. Separately, a market named __proto__ replaces the Store dictionary
prototype and ticker symbol id resolves through that object; Rust misses it.
No venue-local write shim reproduces the shared prototype replacement.

Both gateway-info alias and fallback Store alias keep source before/after views;
Rust returns owned snapshots. Empty/non-JSON bodies, malformed array shape,
non-string errors and raw lone-surrogate failures retain separate findings or
UTF-8 choices. Two pending-read drops release the transport future without
logging. No production Store mutation or lifecycle task is introduced.

The source parseFloat-to-Number control fails child101 at Spot ticker native
bits. A plausible Rust unscaled changeRate output COMPILES and fails101 at the
same named bit assertion; restored. A tracked-source /private/tmp mirror passed
three cases (including two inherited tickers and actual Axios raw bodies) with
no ancestor node_modules; missing package/compiler each failed MODULE_NOT_FOUND.
Both missing-dependency controls are maintained in the Rust test. Mirror removed.

Merged-state candidate at integration17f785479a:91 venue tests passed, zero
ignored; host strict clippy and separate wasm strict clippy passed; parity7334
rows/3200 public items; wiring251, layers38. Extractor zero drift after incoming
Coinbase formatting fix, still local smol-toml evidence rather than clean-install
proof. Merge retains CI90/90->90 and333 parsed exemption blocks/333 distinct,
with exact parent records/reasons retained and unchanged own Cargo dependencies.
Slice plus merged-state adversarial review requested before readiness.

Initial ticker review found that inherited-output refusal preceded numeric
coercion. Corrected to evaluate last, bid, ask, percentage, volume and quote
volume before typed publication.72 live cases span all six fields and all twelve
inherited names with a non-callable toString. They also caught Display adding a
TypeError prefix where source emits err.message; the code now uses the shared
message field. Nine slice tests pass. Restoring early refusal after that passing
baseline compiles and fails the named coercion-before-output assertion; source
and Rust field values identify the actual refusal mismatch, not a build error.

Second review extended the same ordering issue across the whole reduce batch:
an inherited row followed by null must reach the null-row failure first.24
additional cases span all twelve names and a later null/coercion error. Typed
refusal is now deferred until every row has executed. Ten slice tests pass;
restoring per-row refusal compiles and fails101 at `ticker later-row failure
precedes output refusal`. No count advancement before the returned re-review.

Final re-review returned MERGE for the ticker slice and previously checked
merge. Ten slice tests and separate host/wasm strict clippy pass after both
ordering repairs; full suite previously91 tests, with two added ordering tests
passing in the scoped reruns. Measured62 ported/33 divergent/96 pending.

## Pending Spot fills request boundary

Four reference tests pass: 21 native query cases over from/to/limit and NaN,
infinities, signed zero and ordinary values; exact query text and bits before
JSON projection; mapper bits, own undefined keys and clock-call ordering; loud
missing Axios/compiler failures. The live parseFloat-to-Number source mutation
fails101 at `live fills native mapper bits`. This is a held source/representation
witness, not Rust fetchHistoricalFills parity. The row stays pending and measured
counts stay62/33/96. Shared JSON capture correctly refuses NaN; query transport
is the routed owner, and no local shim is added.

Adversarial review returned MERGE for this held witness slice. Host strict clippy
and all251 wiring tests pass. A tracked-source mirror outside the checkout passed
the exact three-spelling query with the existing isolated install; missing Axios
and compiler both failed MODULE_NOT_FOUND. An initial incomplete mirror omitted
packages/logging and failed loudly; the corrected mirror copied that tracked
source too, passed, and was deleted. No new dependency selector is needed. No
production Rust changed; this does not advance the pending row or claim a new
wasm runtime measurement.

## Existing market read refusal-order audit

The pre-repair implementation failed the named source-error assertion: a null
symbol hid a later numeric coercion error.23 live cases cover three refused
string fields against four numeric fields, nine later-row errors, and two
interpolation errors. Numeric and interpolation failures emit the shared message
without its Display prefix. Refusal waits until the entire batch completes.
The old JSON.parse(raw) fixture was replaced by actual isolated Axios; empty and
non-JSON raw bodies now explicitly witness the pending shared response staging
finding. This row was already divergent; measured counts remain62/33/96.

Both restored early and restored per-row output refusals compiled and failed101
at `market source error precedes typed output refusal`, then were restored.
Full scoped suite:100 tests,zero failed/ignored. Host strict clippy and separate
wasm strict clippy pass; local extractor zero drift (local smol-toml caveat).
Tracked-source mirror:three source cases and two loud missing-dependency
controls passed outside all ancestor node_modules, then the mirror was removed.
Adversarial review independently probed both error orderings and Axios raw
fallbacks and returned MERGE. No model widened and no shared shim introduced.

## Earlier REST oracle response-staging sweep

Three more REST oracles used JSON.parse(raw) as an Axios substitute: accounts,
futures order reads and Spot active-symbol discovery. They now execute installed
Axios for raw successes and, where read, raw failure bodies. The numeric-ingress
oracle's intentional JSON.parse source decoder is unchanged. Eight account and
12 order-read cases witness source fallback/transport messages versus Rust decode
diagnostics, retaining request counts and zero order clocks. Five already-
divergent rows receive a separate response-provenance FINDING; the account rows
move CHOSEN to MIXED, with total62/33/96 unchanged. Two discovery cases converge
on an empty array, so discovery gains no finding.

All21 tests across these three targets pass. Restoring JSON.parse separately in
each oracle makes its named raw-response test fail101; these are oracle-fixture
controls, not Rust production mutations. Missing runtime/compiler fail loudly
for every oracle. Host strict clippy, parity7334/3232 and local extractor0 pass;
extractor remains local smol-toml evidence. Production Rust is unchanged, so
separate wasm lint from506a515376 remains applicable rather than being rerun.

Adversarial sweep review returned MERGE and independently probed account and
symbol-discovery outcomes. All three oracles pass in a tracked-source mirror
with no ancestor node_modules; six missing runtime/compiler controls fail loudly.
The initial mirror omitted tracked packages/math and failed loudly; the corrected
source set includes math and logging, passes, and was deleted. No selectors or
production dependencies changed.

## Spot cancelAllOrders candidate

Five tests cover18 live source cases, response/emitter/UTF-8 findings, two drops
of a pending unlimited DELETE, and missing runtime/compiler refusals. Exact
request/emit/rejection event order is compared. Source rethrow-to-return mutation
fails101 by name; a Rust missing-emission mutation compiles and fails101 at
`live Spot cancel emission and rethrow`. Original RestFailure code, context,
request and response survive even when emitted body text differs from its message.
Tracked-source mirror outside ancestor node_modules passes three cases and two
missing-dependency controls; deleted afterwards. No retry or Store write added.
Candidate62/34/95 awaits merged-state review; published62/33/96.

## Field identity sweep

The owned-fixture consumer audit found no outside-crate feeder by tracked filename
and script/test path searches, independently repeated by a sub-agent. Both JSON
goldens remain byte-identical; the numeric corpus was not regenerated. Eleven
crate-local test targets strengthen distinct field inputs/assertions. See
FIELD_IDENTITY_AUDIT.md for the measured census, source constants and explicit
reference-only boundary.

Five compiling swaps returned 101: ticker id/symbol (`live Spot ticker output`),
mapper postOnly/stop (`KuCoin mapper numeric field identity`), token-server ping
interval/timeout (`direct KuCoin numeric ingress refused`, `$.pingInterval`),
balance witness amount/notional (`balance coin field identity: BTC`) and fills
witness orderId/commissionAsset (`live fill string field identity: orderId`).
The last two mutate Rust consumers of source output, not pending production reads.
Mutations are restored; the first eight repaired targets passed 45 tests and the
three added reference/validation targets passed 14 before the merged-tree run.

The merged tree with `a3d514bec8` passes 109 venue tests across 24 binaries, zero
ignored; host strict Clippy and separate wasm32 strict Clippy both exit 0.
Parity passes 7,334 rows / 3,234 public items; wiring passes 251 assertions;
layer audit classifies 38 crates; extractor reports zero updates, additions or
orphans. Extractor resolution from this worktree's installed smol-toml is local
validation, not a new clean-install claim. The cancellation manifest's missing
spot_writes.rs file declaration was caught by parity and repaired before its rerun.
CI step multisets retain 90/90 parent steps in 90 merged steps. A real tomllib
parse reports 338 exemption blocks / 338 distinct targets, zero blank reasons,
with every parent entry retained. Own Cargo manifest and lock are byte-identical
to both parents. The final baseline capture labels and excludes mutation runs;
85 formerly identical-only record-path pairs now have differing cases, counting
input/output paths separately. Both goldens and the numeric corpus are unchanged.

## Spot order pagination and concurrent rejection witness

The pending fetchOrders row has a source-only witness executing actual venue
pagination, discovery, mapOrder, Store and installed Axios response transforms.
Seven scoped tests pass, including first-page invalid totals, later retained
and changing totals, the 100-page cap, duplicate/non-string discovered symbols,
input-order flattening with reversed completion, fallback array/info aliases,
and sibling sends/emissions after the outer fallback returns. A later rejection
is explicitly observed after return, rather than inferred from request counts.

Three live-source controls fail with exit 101: sequential fanout names the
missing concurrent request 1, empty fallback names the current-Store assertion,
and removed page guard names the retained-totalPage assertion. A compiling Rust
reference-consumer amount/filled swap fails `live Spot order numeric identity:
amount`; restoring it passes all seven tests. This is a witness-consumer control,
not a claim to have implemented the pending production read. Captures canonicalize
NaN only; native finite values and signed zero retain bit identity.

The raw-body cases execute actual Axios for JSON, empty/non-JSON success and
failure bodies, including body-message precedence. Fixture dimensions are
crate-local, distinct and asserted; they feed no outside corpus. Existing
cross-crate feeder artifacts are not edited. Shared owned aggregation remains a
routed requirement: raw runtime spawning is not a venue lifecycle contract.

Host strict clippy and separate wasm strict clippy pass for the new test target.
CI wiring passes 251 assertions. A tracked-source mirror with no ancestor
node_modules passes a raw JSON source case using the existing isolated install;
missing Axios and compiler controls each fail MODULE_NOT_FOUND. The mirror was
removed. This reuses the previously proven install, not a fresh-install claim.
Sub-agent re-review returns MERGE after the three witness gaps were repaired.
Counts remain 62 ported / 34 divergent / 95 pending by tomllib; this slice
advances no production row and is not a new merged-state readiness claim.

## Existing Spot mapper refusal ordering audit

Sixteen malformed id/symbol cases across eight source coercion fields and nine
inherited-output/later-price cases pin exact error.message before a typed
representation refusal. postOnly=true separately bypasses a throwing TIF value.
The oracle now captures error.message rather than Display/String(error), matching
the emitting callers; ten futures coercion cases audit the shared helper change.
Equal coercion messages do not prove which of two coercions ran first, and these
cases make no such claim. Source order is retained structurally in the mapper.

The old early identity checks restored after remaining and before TIF compile,
then fail with exit 101 at `Spot mapper source error precedes typed refusal`:
Rust reports the id refusal where the source reports its TIF coercion error.
This covers one mapper invocation only; no whole-page/concurrent-read timing is
claimed and neither divergent mapper row advances status.

Scoped mapper/order-read tests pass 9+7, and the added stopType field case passes
in the ten-case futures message corpus. Host and separate wasm strict clippy
pass for all own-crate targets. Extractor reports zero updates/additions/orphans
through the real Yarn entrypoint (local install caveat retained); layers reports
38. Sub-agent implementation review returns MERGE. Counts stay 62/34/95.

## Notification cutover slice

Six production sites are inventoried in STORE_NOTIFICATION_AUDIT.md, including
Spot cancellation omitted from the original five-site list. The local merge of
af44700b retains 90/90 named CI steps and 359 blocks/359 distinct parsed
exemptions with every parent reason unchanged; the layer audit passes.

The reporting oracle executes live venue/reporting code and real TinyEmitter:
54 cases, distinct first TypeError and second RangeError, request counts and
ordered fanout. Six scoped tests pass. Three production mutations compile and
exit 101: early propagation fails `fallible emission source ordering`; retaining
the first error fails `original listener error metadata retained`; flattening
StoreMutationError fails `notification origin lost`. Every restored test passes.

The isolated frozen-root-lock install appends only tiny-emitter from the owning
package. A tracked-source mirror with no ancestor node_modules passes 54 cases;
missing runtime and compiler controls each exit 1 with MODULE_NOT_FOUND. The
mirror is deleted. The sub-agent review found no remaining implementation blocker
after distinct-error, subscriber-id and empty-raw fixture repairs. This is slice
evidence, not a full merged-state readiness claim; raw error-value and response
provenance findings remain separate.

Integration merge input `9af82bfeae`: 90/90 to90 named CI steps,359/359 parsed
exemptions, no missing parent entries/reasons, own Cargo manifest unchanged.
Scoped lib check passes. The first all-targets host lint pass found a redundant
closure in the pending numeric-capture sweep; after fixing it, the second pass
exits0 with zero new unused Results or type errors. The six-site inventory is
not inferred from the first compilation alone.

The actual-method cancellation cases pin both precedence directions at
`cancel failed emission replaces original transport failure` and
`cancel successful emission preserves original transport failure`. The latter
asserts concrete RestFailure code/context/request fields, not a string projection.
The reporting target also retains distinct store_subscriber_ids lists on its
first and second listener errors. Three targets pass eleven tests total.

## Local NaN and native identity follow-up to pinned PR 7253

PR 7253 remains pinned at aedafd658d; its migration-only verdict does not approve
the earlier sweep WIP. This follow-up is local and has not been pushed.

Four native-identity targets pass 14 tests. A symmetric serde endpoint/protocol
name swap compiles, fails with exit 101 at `native server endpoint identity`,
and passes after restoration. The NaN capture helper covers eight explicit
patterns; separate compiling zero-sign and finite-value mutations fail with exit
101 at `signed-zero capture identity` and `finite and infinity capture identity`.
Each restored target passes. Local capture metadata is arm64 / Node major 26;
no x86 CI execution is claimed.

Eleven per-case loops were batched from 80 to 11 baseline Node invocations across
accounts, order_reads and spot_markets. Those three targets pass 24 tests.
FIELD_IDENTITY_AUDIT.md distinguishes the historical 31-artifact census from the
current 35-artifact inventory, retains both original golden hashes, and records
no outside-crate feeder found by the tracked-consumer search. Manifest counts
remain 63 ported / 33 divergent / 95 pending, parsed from all twelve manifests.
This is scoped evidence pending the complete sweep review, not full-head approval.

The full scoped target pass reached 120 tests across 25 integration binaries:
22 binaries/105 tests passed before validation's new capture metadata exposed a
consumer comparison mismatch. Validation now asserts architecture and Node major
separately from venue output; its eight tests and the two remaining binaries
(15 tests total) pass. No failed result is counted as a pass.

The derived-precision audit then reproduced a surviving bracket-down production
mutation. Twelve fractional, distinct-step cases and native payload assertions
repair that blind spot. Bracket-up, bracket-down and algo-stop price-to-amount
mutations each compile and exit 101; bracket-down names
`native KuCoin precision identity: triggerStopDownPrice`. Each restored target
passes. The previous bracket-down exit 0 is retained as false-green evidence.

After the two test-consumer changes, host and separate wasm strict all-targets
clippy pass again, crate fmt passes, wiring passes 251 assertions, and layers
passes 78 tests. Global parity passes 7339 rows / 3322 public Rust items. Real
TOML parsing gives 359 exemption blocks / 359 distinct targets / zero blank
reasons. Extractor remains inherited red at integration 710e663156: two added
Chaser symbols and bingx.api formatting, three manifests, zero orphaned symbols.
Neither those shared manifests nor the pinned PR head were edited to hide it.

## Symbol cancellation slice

The source oracle executes cancelSymbolOrders and the actual Store constructor.
Forty-nine cases pin own market id, empty-id fallback, unknown/Unicode/empty
symbols and all twelve stock inherited names, with successful, bad-code and
transport responses. A separate prototype-replacement witness shows source
info.id redirecting the request to poison-wire while Rust uses info. This remains
a shared Store write FINDING, not a reproduced security-relevant mutation.

The reporting corpus expands from 54 to 60 cases, retaining listener failure
precedence and original transport metadata for symbol cancellation. Eight cancel
tests plus the notification target pass; the added two-case lone-surrogate
adapter-parameter witness separately passes its ninth cancel test. It does not
claim actual Axios query transmission for lone surrogates. A production market.id-to-symbol swap
compiles, fails 101 at live symbol cancel wire identity and error ordering, and
passes after restoration. Pending symbol DELETE is dropped twice; the Store can
be updated while it is suspended, pinning borrow release before the await.

The first host/wasm lint exposed a Store RefCell borrow held over the await. An
owned symbol selected inside a lexical borrow scope repairs it; subsequent host
and separate wasm strict clippy pass. No suppression or lifecycle task is added.
This is slice evidence, not a new whole-head readiness sheet. At that checkpoint, create price/create size/algo size precision mutations
remained outstanding; the continuation below records their later checks. The
prior three-field result was not a venue-wide precision claim.

## Repaired integration and per-field precision continuation

Local merge 7585fd5a1a consumes integration e53d89edde, including the MEXC
fallible-log propagation. Both parents have 90 named CI steps; the merged tree
retains all 90. Real TOML parsing retains 359 blocks / 359 distinct exemptions
and every full parent record. KuCoin dependencies are byte-unchanged. Its scoped
--tests check and the 78-test layer audit pass; this is not a workspace build.

The formerly outstanding three precision fields are now audited alongside the
previous three, each with a distinct compiling mutation and named assertion.
All six exit 101 and all restored targets pass. FIELD_IDENTITY_AUDIT.md records
which historical claims are known and which are current-corpus claims only,
plus the original canonical-input count/hash and thirty-case labeled supplement.

Merged-tree scoped validation after that continuation passes 124 tests across
25 integration binaries in one run, zero failed/ignored. Host and wasm strict
all-targets clippy pass separately, as does crate fmt. Global parity passes
7339 rows / 3361 public items. Layer evidence is the fresh 78-test pass on this
merge; the 251-assertion wiring inputs are unchanged from the prior measured
sheet. The six per-field mutation logs name their own KUCOIN-PRECISION fields.

## Sequential Spot cancelOrders: one pending row moved

Tested code SHA: `2cf0ffc5e550d33d8cd9617e700d283d71b8097f`.
Implementation commit: `5757986beccde4f6e07b48d0db8c44cf9cf3f4cc`.
Actual inherited parent: `6056179a6b9b7ed351e3e4e3551171db8bc65b66`,
whose parents are `4543d553119c306699a41f268cefbf0d6cdac736` and
`e09e62df48ed594e7e91403674c839a9f870e545`. This slice did not merge newer
integration. Against refreshed integration `2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`,
the inherited KuCoin source/manifests have no diff, and the only unique ancestor
is the local merge `6056179a6b`; no unreviewed feature implementation was adopted.

Python 3.11 `tomllib` parsed all twelve KuCoin manifests at the named integration
SHA and in the restored tested tree. Baseline: 63 ported / 34 divergent / 94 pending.
Result: 63 ported / 35 divergent / 93 pending. Moved **0 pending to ported,
1 pending to divergent**, specifically `KucoinSpotExchange.cancelOrders`.
The row is MIXED: sequential requests are implemented; shared UTF-8 and owned
alias choices, Store prototype writes, response provenance and raw emissions
remain documented boundaries. No pending query ABI was adopted.

Measured gates on the tested code SHA:

| Command / scope | Exit and measured result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 0; scoped host compile, zero errors |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test spot_cancel --test notification_reads` | 0; 13 cancellation tests and 1 reporting test passed, zero failed/ignored |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; output independently inspected: 7341 symbols / 512 files, zero changed manifests, added or orphaned symbols |
| `rustfmt --edition 2024 --check` on the three changed Rust files | 0 |

The extractor writer was run after editing the scalar manifest mapping and
before committing: exit 0, updated zero manifests, zero added/orphaned symbols.
The subsequent check above covers the committed mapping. An initial scoped
check at `5757986bec` failed with exit 101 on a test fixture's unparenthesized
array-index expression inside `json!`; `2cf0ffc5e5` fixes it. That failure is
not counted as a passing gate.

One deliberate compiling production control replaced `order.id` with
`order.symbol` in the cancellation URL expression, leaving the fallback alone.
On the tested SHA plus that temporary mutation,
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2
--test spot_cancel sequential_cancel_pins_request_identity_and_stops_at_first_error
-- --exact` exited **101** at `KUCOIN-SEQUENTIAL native order id`:
native URL `/api/v1/hf/orders/BTC-USDT` differed from independently expected
`/api/v1/hf/orders/first/17`. A `finally` restored the source byte-for-byte;
the same command then exited **0**, one test passed. No control survived;
only this one new production mutation is claimed.

Native method, URL and symbol assertions run before request JSON capture.
Native listener errors and original transport metadata are asserted before
event projection. The eight sequential parity cases stop at index 7, with
literal request counts `[0,2,1,1,2,2,2,2]` and twelve checked requests. Four
boundary witnesses stop at index 3, and one surrogate witness at index 0.
First and second suspended awaits are each dropped twice, stopping at request
2 / repetition 2. The reporting corpus has 66 cases, including six entering
cancelOrders; each of those has an independent native one-request assertion.
These enumerations are floors, not exhaustive runtime acceptance claims.

A private sub-agent performed static review only. Its finding about native
errors following event projection was fixed in `2cf0ffc5e5`, and its reread
confirmed resolution with no further defects found. It ran no cargo or gates.

Not run for this slice: complete venue oracle, workspace compilation, host
Clippy, wasm compilation/Clippy, parity Rust tests, or CI. The historical table
mutation/census findings above were not remeasured. This evidence covers the
named code SHA; the following evidence-only commit adds this record and does
not claim new code validation.

## Native reconnect dispatch

The two `native_reconnect_tests::typed_*` regressions construct KucoinExchange
with NativeRuntime, NativeHttpHost, the reqwest adapter and real loopback HTTP
and WebSocket peers. Both futures sockets acquire a token, connect, receive a
typed reconnect, synchronously clear source session fields, obtain another
signed bullet response and dial with its fresh token. The expected reset fields
come from executing the maintained TypeScript reconnect method bodies in Node.
`tests/reconnect-controls.py` runs both tests against pre-fix master production
files, requires the named token-reset failures, restores exact source bytes and
requires both tests to pass again.

Four additional native tests deliver private orders, match/fill, position and
wallet frames through the actual binding. They assert the order Store and bare
incremental fill event. Position/wallet frames trigger the source's delayed
orders-only snapshot and preserve seeded positions/balance, matching the
inherited TypeScript BaseExchange.fetchAndSetPrivateSnapshot implementation.

## Production order-reader cleanup (2026-10-18 SGT)

The old src/order_reads.rs implementation, its eight tests, source-only oracle
and cardinality control are retired. Earlier measurements of that copy above
are historical, not production coverage. The constructed Exchange installs
futures_native_order_reads through binding/futures.rs. Its retained component
tests remain; tests/order_reads_binding.rs additionally drives actual HTTP
requests and the real Store. Redirecting the live normal reader to the algo
endpoint compiles, leaves all eight old copy tests passing, and fails the new
binding witness at duplicated algo IDs. Exact restoration passes. The new
witness also checks ordered endpoints/params, native contract-size scaling and
failed-normal/empty-algo Store category preservation. This deletion changes no
production algorithm or shared representation policy.


## Production Spot I/O cleanup (2026-10-18 SGT)

The parallel typed spot_reads/spot_writes implementations, their four test
targets and exclusive source oracles are retired. Earlier measurements of those
copies are historical. Production already installs spot_native_order_reads
through spot_exchange_class; its native/source component cases remain. New
tests/spot_io_binding.rs drives the actual constructed Exchange, HTTP factory
and Store for market precision, distinct ticker prices and fallback, active
symbol discovery/paging and three cancellation routes with exact params. Three
compiled mutations independently change the live ticker bid field, discovery
endpoint and cancel-all endpoint. Each leaves its corresponding retired-copy
tests passing but fails the constructed witness. Exact restoration passes all
three constructed tests. This is deletion of unused code, not an algorithm
cutover or a resolution of shared typed projection/identity policy.


## Production account-copy cleanup (2026-10-18 SGT)

The unused typed accounts module and its exclusive response_default parser are
retired, with their three account/validation/notification test targets and
source-only oracles. Earlier measurements of those copies are historical. The
constructed Futures and Spot exchanges already use the retained native account
modules; their existing component/source cases remain. The paired package and CI guard now include exchange_trait in the venue oracle command. Changing only the production futures userId read
to referrerUserId compiles and leaves all eight old typed account tests passing,
but fails futures_binds_all_sixteen_defaults at empty ID versus user-1. Exact
restoration passes. The actual constructed test and its runtime fixture are
unchanged, preserving concurrent private-topic work. Canonical shared transport
parsing and all shared body/API/representation policies remain unchanged.

## Typed trading-copy retirement (2026-10-18)

Production class dispatch uses the native mapping/formatter/helper bodies. The
unused typed mappers, formatters, order_helpers and seven sizing functions are
removed with five exclusive test targets, four source oracles and formatter-only
cardinality controls. Nineteen decoder contracts owned only by those copies are
removed; all surviving contract objects are unchanged. Live typed to_api_symbol
and api_margin_coin stay, and the sizing source extractor retains the two methods
needed by ws_helpers. No native implementation or shared model API changes.

Before deletion, a compiled actual native amount mutation leaves all ten typed
mapper tests green but fails constructed order_reads_binding. A compiled native
buy/sell mutation leaves all four typed formatter tests green but fails the
constructed futures exchange_trait request payload assertion. Both witnesses pass
after exact byte restoration. Existing native/source helper, mapping/formatter,
order-read and constructed exchange targets remain wired. The Order snapshot
supplement retains every assertion while dropping an empty-Store identity call
to the retired helper. Earlier typed-copy counts and refusal findings in this
file are historical evidence, not coverage of the installed production route.

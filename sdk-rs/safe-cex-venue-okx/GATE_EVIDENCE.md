# OKX gate evidence

## Protocol tables checkpoint

The mandatory maintained TypeScript oracle compares all 13 intervals, all 28 endpoint entries, both region and instrument-type unions, all global/EEA live/demo REST and socket hosts, telemetry lists, and order/side mappings. The compiler evaluates the original domain enum declarations; values are not duplicated into a JavaScript fixture. Both a Rust `2h -> 1H` mutation and the corresponding maintained TypeScript `2h -> 1H` mutation fail `maintained_typescript_protocol_tables` at exit 101. Original sources restored.

Baseline: scoped cargo fmt, plain strict clippy, live Cargo table test, parity invention/status, and the 49-test CI/layer structural audit pass. CI controls remove the OKX step or its `--include-ignored` flag and are rejected by name. Remaining exchange/socket rows stay pending; this checkpoint does not claim their behavior or lifecycle coverage.

## Pure ledger/history/closed-position checkpoint

`venue-oracle:okx` runs the maintained TypeScript mapper functions and their actual private numeric helpers against 1,012 inputs. It compares values and field presence, canonicalizing JSON number spelling to compare exact binary64 values without tolerance; it does not claim signed-byte coverage. The only accepted differences are the explicitly marked prototype-key enum corrections, with 153 corrected records/cases pinned. The corpus covers linear/inverse injected conversion, closed-price guard, history reference-price fallback, missing market, malformed/negative time, terminal cancellation, and every declared enum/table value.

Negative controls (each fails `maintained_typescript_ledger_history_and_closed_mappers` at exit 101): remove Rust residual emission; replace the TypeScript relative epsilon with fixed 1e-12; change Rust canceled history to Closed; correct TypeScript's inherited-key bill lookup upstream. Original sources restored. Mandatory root-script CI invocation, removal/masking controls and exact root command are pinned by the 50-test CI/layer audit. Scoped fmt, plain strict clippy, wasm build, extraction freshness and invention/status pass for this checkpoint. Exchange/socket runtime rows remain pending.

## Accepted input sets and shared coercion repair

Both public literal unions previously accepted externally tagged JSON objects.
String-only decoding now delegates to PRIVATE derived wire enums, with canonical
wire literals declared only there and checked-in public declarations visible to
the invention gate. No permissive inherent public decoder, FromStr or public
conversion from wire input is exposed. Public qualified-call regressions reject
objects; the live gate rejects arrays/null/booleans/numbers, unknown literals,
case changes, whitespace and U+FEFF. Injecting an alias in the actual
RegionWireSchema decoder fails
`derived_accepted_sets_match_typescript` at 101 with OKX_ALIAS explicitly named.
Restoring default wide decoding fails the protocol table gate on {okx:null}.
The draft anonymous TickerMarkInput also now rejects positional arrays and null
on optional non-null fields through its only public qualified decode path.

The utility merge 89d6420002 retains BOTH parents' named CI steps: OKX 41 and
utils 40, merged 41, with no missing names (all workflow names: 48/47 -> 48).
The additive exemption union parses as 209 entries, no duplicated rs and no
blank reasons, retaining all 202 OKX and 209 utility-parent paths. Each conflict
hunk was resolved independently; numeric spelling delegates to the lower shared
implementation rather than keeping a second formatter.

All local String/parseFloat/parseInt implementations are removed. Source Number
conversion sites retain number_from_string; mapper numeric prefixes use shared
parse_float and timestamp sites use parse_int with explicit radix 10. All four
mapper APIs propagate JsCoercionError as Result. The ledger/history/closed corpus
now contains 1,178 cases: previous 1,012, five scalar throwing objects, 160
throwing objects injected into each coerced row field, and the exported bill
filter constant. The journal corpus contains 310 cases: previous 230 plus 80
field coercion errors. Normal prototype corrections and source-bug pinning remain
unchanged; the journal JSON projection preserves prototype serialized shapes.

Replacing Rust's propagated coercion error with unwrap_or_default compiles and
fails the mapper gate at 101 on expected Err vs Ok(0). Replacing source coercion
with an own-toString object substitution fails the source error-name assertion
at 101; a first overbroad mutation also changed valid array conversion and was
replaced with that targeted control. Original sources restored.

The full OKX suite also runs using SAFE_CEX_TYPESCRIPT_PATH pointing at an
isolated TypeScript-only dependency directory. No tsx/ts-node dependency is used
by any OKX oracle; the maintained CI root command includes ignored tests and the
existing isolated compiler environment. Runtime exchange/socket rows remain
pending; these pure-policy and mapper results do not claim their lifecycle.

Restored checkpoint at 3f422e4bb6: all 9 test functions pass (including ignored
live source cases), scoped fmt, strict all-target clippy and wasm build pass.
The 52-test CI/layer structural audit passes with zero skipped tests; extraction
check reports zero changed/added/orphaned rows (6,901 total), and invention/status
passes with 1,108 public items in the merged workspace. The enum macro was
rejected by the source index and replaced with checked-in declarations rather
than weakening the gate. 29 of the original 163 OKX-family rows are mapped,
plus all three newly assigned journal-adapter rows: 32/166 closed, four declared
divergences. Exchange/socket orchestration and lifecycle rows remain pending.

## Integration merge and market policy

Integration merge a852ccbfd9 preserves named-step OCCURRENCES (including repeated
setup names): OKX parent 56, integration parent 52, result 56. The earlier utility
merge had 56/55 -> 56 occurrences; the 41/40 -> 41 figures above count DISTINCT
names. Both occurrence counters and exemption path sets are checked per parent.
The merged exemption union has 210 tables (209/116 parents), parsed with no
repeated rs or empty reason. Integration scaffold cleanup automatically removed
OKX dependencies; restored only currently consumed types/num/serde/serde_json.
The merge acquires the shared raw JSON parsers and LoadedChunks observability.

Six additional global-base market methods use explicitly supplied receiver data;
the EU overrides remain unclaimed. A 401-case maintained AST-method oracle covers
strict instrument acceptance, truthy info/top-level precedence, fallible String,
underlying index IDs, duplicate/filter order and original-object references,
xperp symbol behavior, BOM and final LF/CR/CRLF/U+2028/U+2029 suffix cases.
State comparison canonicalizes JSON numeric spelling only, with exact binary64
values and field presence; this is not a byte-preimage gate. Reversing Rust's
filter output and removing the maintained TS regex's end anchor each fail
maintained_typescript_market_instrument_and_index_policy at 101, naming the
sequence case and BTC-SWAP-EXTRA respectively. Original sources restored.

Production panic sweep: the only local expect parsed formatter-generated finite
JSON. It now returns a recoverable error instead; there is no demonstrated input
that made the formatter's valid finite output unparsable, so this is hardening,
not a claimed TypeScript defect. All actual source coercion throws remain Result;
metadata inventory assertions live under cfg(test) only. Journal byte oracle
remains green after the hardening. Builds/tests now use --jobs 2 and run serially.


Response-bearing runtime slice: 108 maintained-source cases exercise lookup and
normal/algo placement through the actual HttpClient implementation of RestAdapter.
Only the wire transport, emitter and still-pending mapOrders collaborator are
recorders. Actual p-iteration mapSeries and lodash chunk/flatten execute through
the isolated install; p-iteration's owning package selector was appended without
changing existing selectors. The copied root lockfile install is frozen.
The corpus includes strict string 51603 versus numeric/display-only occurrences,
all terminal lookup states, absent market/store short circuits, original rejected
response status/reason/headers/body, per-order and top-level rejection precedence,
20-item batch boundaries, sequential single algo writes, empty input, first-error
termination, throwing String fields, BOM and overflowing 1e400 response codes.
State numbers compare exact binary64 values; request bodies compare exact bytes
using the shared JSON.stringify primitive, including 1e-7 and 1e21 spelling.
Five Object.prototype lookups and eight malformed native-result cases are
separately source-pinned representation/diagnostic choices, disclosed on the rows.
The native ClientOrderLookup ABI owns a copy on store hit; it cannot claim the
TypeScript reference identity. The pure mapOrders implementation is not claimed.
API factory/signing/retry composition remains pending; only its private response
interceptor is prepared and exercised here.

Permanent controls mutate each source response path in memory and require exit 1
at assertions; an unavailable package mutation must fail MODULE_NOT_FOUND. A
native child substitutes the plausible lossy host boundary (pre_transmit of the
message), requiring exit 101 at lookup error 2. A production mutation suppressing
placement response-body extraction compiled and failed the named live oracle at
placement response body 34 (display text versus per-order venue rejection/code).
The source was restored and all positive cases rerun. These controls prove the
response is consumed, not merely present on a type. No socket or task is spawned
by the operations; caller cancellation owns no shared latch/cleanup resource.

Merge hygiene: response ABI merge retained 56/55 -> 56 named CI occurrences and
210/215 -> 215 unique exemption tables. Canonical primitive integration merge
retained 56/55 -> 56 and 215/228 -> 233 tables; both parent path sets survived,
with no duplicate rs or empty reason. Scoped compilation immediately confirmed
OKX dependencies survived the latter merges, and the layer audit passed.
Global parity-check currently fails with the compile_error diagnostic-origin
index error. The identical error reproduces on a tracked HEAD mirror before this
runtime diff; the tracked integration mirror passes 6906 rows/1110 items. This
is routed to the owner and not suppressed. Extractor --check passed with no
manifest changes. Local native/wasm/clippy results are recorded separately from
that global gate failure; no global-green claim is made.

The tracked-source mirror ran all seven current Node oracles using the frozen
isolated install and no ancestor node_modules; all passed, and three response/
missing-package controls failed at their expected exit-1 assertions/error.
Mirrors were deleted after validation. The OKX gate is in the NATIVE test job,
not the root-installed TYPES job, so the p-iteration selector is required.
The existing OKX ToBoolean wrapper already treated overflowing numbers as true;
it now delegates to the canonical shared js_truthy rather than carrying another
implementation. This is consolidation, not a claimed overflow bug repair.


Public dispatch / sequence recovery slice: 136 ordinary original-source scenarios,
plus a separately asserted FINDING for raw seqId:1e400. Shared HOW null projection
changes snapshot acceptance into unsubscribe/subscribe recovery; no venue-local
parser is introduced. The two prototype function handlers are source-pinned
CORRECTED cases through the chosen core typed registry. Gap latch, recovery
snapshot, first callback error, captured-list removal, stats-before-callback,
reentrant recovery send and first-send failure are compared explicitly. Every
case disposes the socket and drops host tasks, then requires Weak release.
Permanent source gap-guard and prototype-hardening mutations fail at assertions.
Removing the Rust recovery latch fails at recovery frames 0, exit 101, with two
recovery pairs versus one; restoring it passes with the isolated compiler.
All 13 scoped tests (including ignored oracles), strict clippy and wasm check pass.

Named constant-table lookup inventory: 26 AST element-access sites, including
25 in the venue and one in the journal adapter. BASE_URLS 1, BASE_WS_URLS 3, ORDER_SIDE 8,
ORDER_TYPE 2, ORDER_STATUS 2, INTERVAL 2, OKX_LOOKUP_LIVE_STATE 1,
OKX_LOOKUP_STATUS 1, OKX_BILL_TYPE_BY_INCOME_TYPE 1, REVERSE_ORDER_TYPE 1,
BILL_TYPE_CATEGORY 1, CATEGORY_INCOME_TYPE 1, TIMEFRAME_BY_CANDLE_CHANNEL 1,
CLOSE_REASON 1. BILL_TYPE_CATEGORY uses || other; both historical order tables,
both lookup tables and CLOSE_REASON use nullish fallback. fetchExecutions tests
ORDER_SIDE truthiness and business onMessage tests timeframe truthiness. Other
sites feed values without || fallback; trailing/reduce-only side sites additionally
feed strict side equality branches. Typed caller region/interval/order inputs
are distinguished from upstream row/channel keys. Already-claimed mapper and
lookup prototype cases are pinned; pending runtime sites remain unclaimed.


ABI refresh: integration merge 4b7482a680 retains named CI multiplicities
56/66 -> 67 and exemption path union 233/254 -> 259. Core merge 683a93d5eb
retains 67/59 -> 71 and 259/247 -> 265, includes required-clock retry ABI,
response code and boxed context, RestBody origin and owned prepared socket start.
Immediate scoped compilation caught and repaired fixture API changes; actual
OKX dependencies remained intact. Both layer audit runs pass 78 tests.
The 108-case REST response witness and its lossy-boundary control now pass on
this ABI: actual response string 51603 and sCode/sMsg remain distinct from
display text; placement carries already-encoded source JSON bytes explicitly.
No RetryPolicy is constructed in the currently mapped slice (factory pending).
HOW fixes e82cd657ea/f5f1716b33 were not yet in integration, so only those two
authorized commits were taken, with the outbound test updated to RestBody.
The OLD overflow-finding expectation fails matched-key callbacks/data/stats 136
(exit 101): native now calls books.BTC instead of initiating recovery. Removing
the finding profile retains the original-source Infinity sequence assertion and
the socket oracle passes; overflow is now a regression, not a declared finding.
Source prototype dispatch remains a distinct FINDING under the new preservation
ruling; the source witness and hardening control are retained.

The handler sweep now exercises every Object.prototype own name: eight
positive-arity functions record stats and throw; three zero-arity methods and
the prototype object return at the length guard. typeof and arity are captured
before JSON serialization. The 144 scenarios include eight explicitly retained
FINDING profiles and 136 source-matching cases, not 144 parity claims. The upstream
prototype-hardening child still fails at its named source assertion. Recorder
ordering is pinned; this does not claim the separate core stats body's inherited
member representation, which is routed because reconciliation rereads its registry.

## Resume: staged integration and response-phase finding

Four merge commits 3f5cd5598f, 9ff003c63e, e1d6f32624 and 3e3491a93a consume 6bcff79504, 8cbfb8b5f9, b8fc4dfbaa and current remote 56e818d52b. Each passed scoped venue tests including ignored live source oracles, immediate all-target check, separate host/wasm clippy, parity, layers and wiring. CI parent multiplicities were76/78→79,79/80→81,81/82→83,83/86→87, none missing. Real tomllib parses were303/303,303/303,312/312,312/312 blocks/distinct with no blanks or missing parent targets. Final wiring234 passed after adding the existing worker prerequisite check before the cancellation check; no worker assertion was weakened. Extractor0changed/0added/0orphaned uses existing extractor-local smol-toml and does NOT close the clean-install wiring gap in RESUME.md.

Actual createAPI plus common Axios factory and production bigInt HOW run over recording I/O/host hooks. Fourteen direct/HOW responses retain typeof and number bits before JSON serialization. Direct quoted123 staysstring; HOW promotes tonumber bits405ec00000000000. Quoted true/null promote too; hello/object/array/null controls agree. Core-owned RestResponse phase/type provenance remains missing; createAPI and its constructor caller remain PENDING with FINDING notes. There is only one factory row in the maintained OKX family. No local parser shim or completed runtime factory is claimed.

Source control bypassing HOW customParser fails exit1 at source HOW double parse retains response phase. Missing axios-retry import fails MODULE_NOT_FOUND. The Rust mandatory venue test runs baseline plus both children. A disposable tracked-source mirror with no ancestor node_modules ran all three with identical baseline bytes and was deleted. Runtime packages come from the existing frozen isolated selector install; no selector changed. The prototype documentation now reflects pending source-preservation work and the closed-output declaration, not the superseded fallback correction.

Fourteen-case response oracle SHA256: a97dda50e54bfb50a519866516178388f2983d88a822af2953c87f6fdabdab18.

## Shared numeric ingress consumed

Merge e56ef3f835 consumes integration b283acad08 (including e5f0069ae1). Tail union retains87/87 parent steps→88, with323 TOML blocks/323 distinct, no blanks or missing parent targets. Scoped venue tests, both clippy targets, parity,78layers and241wiring assertions pass; extractor remains locally installed, not clean-install proof.

TickerMarkInput now captures JsNumberInput at its own whole entry and decodes its private wire through the map-only guard. Explicit JsNumberOutput registers id/symbol/mark through the exhaustive expression macro. The existing owned/borrowed integer -0 witnesses FLIPPED and now require the source sign. Raw/owned/borrowed nested and internally-tagged wrappers use the public shared whole-entry APIs; negative underflow and both overflow signs retain bits. No local numeric shim or Value-first conversion exists. Replacing the decoder with its old Value-map buffer COMPILES and fails ticker_mark_ingress_retains_integer_negative_zero at the named owned-Value bit assertion; restored baseline passes.

Source witnesses assert the actual lone UTF-16 code unit before serialization and retain ordinary scalar/astral/BOM positives; Rust refuses that valid JSON escape under the canonical UTF-8 declaration. The source helper also ignores a140-deep extra field that bounded ingress refuses. Its row remains divergent as MIXED with those two CHOSEN boundaries, not the repaired numeric FINDING.

### Inherited dispatch after the core stats read landing

The earlier 144-case oracle explicitly replaced the eight positive-arity source TypeErrors with success. It could therefore agree with a Rust implementation swallowing those errors; its green result did not establish parity for those eight outcomes. The other 136 cases were not normalized. The oracle now retains the actual TypeErrors and compares them before serialization. All twelve prototype names remain in the corpus (eight positive-arity, three zero-arity and the prototype object). A compiling Rust mutation changing the positive-arity threshold to greater than two fails the named frame-result assertion at exit101. Core stats-before-error and own-empty shadowing have separate regressions. Global inherited-referent mutation remains CHOSEN FINDING, not serialized convergence.

A second compiling control suppressing only the core inherited-stats call fails inherited_error_records_core_stats_before_returning at its named arity-before-TypeError assertion (exit101); this proves the invocation is required rather than merely present. Restored dispatch baseline has three passing tests; the cancellation gate retains six passing tests plus the explicit ignored export and both waiter regressions pass.

### First public Store writers

Open-interest and funding-rate handlers have 51 actual-source cases spanning oiCcy, fundingRate and fundingTime independently. Exact f64 bits are compared before JSON serialization, including -0, negative underflow, overflow, BOM, non-JS whitespace, prefix parsing, array/object coercion and own-toString TypeError. Rust applies each case through real DefaultStore and an ordinary listener; only the first update is consumed, successful writes notify once and coercion throws notify zero times. The TypeScript handler runs against a recording Store boundary, so this does not claim TS Store implementation or throwing-notification parity. Both rows remain divergent with the notification FINDING and the two call sites are listed in STORE_NOTIFICATION_AUDIT.md. A source wrong-field mutation fails its named oiCcy assertion at exit1; a compiling Rust +1 mutation fails the named openInterest bit comparison at exit101. The corpus assigns unrelated fields different values so wrong-field substitution is discriminated.

### Mark/index loaded-state ordering

The ticker corpus now has 85 actual-source cases over five numeric fields. It compares complete select/loaded/write traces, preserving per-market ETH-before-BTC selection order. Mark-loaded precedes parsing and remains observable when mark coercion throws; index coercion throws before any selector/loaded/write call. A TypeScript loaded-after-write mutation fails the named source-order assertion at exit1, and a compiling Rust loaded-after-write mutation fails the full trace assertion at exit101 even though both operations still occur. The ordinary Store notification and borrowed-ID projection remain explicitly bounded adapted dependencies; loaded-state/selector methods are separately accounted rows. Notification, array-frame and shared prototype-write findings remain on both new rows.

### Main ticker handler

The family corpus now has209 actual-source cases, including all six main ticker input fields independently, signed zero/overflow, first-record selection, throwing coercion and fractional percentage probes. It executes maintained roundUSD, not a recreated helper. A TypeScript currency/contract-volume swap fails the named currency-unit assertion at exit1. Compiling Rust controls separately bypass rounding and substitute quote-volume into volume; each fails its named percentage/volume bit assertion at exit101. The prior51/85 measurements above describe their earlier slices. All five notification writes are audited and explicitly type their current StorePatchError conversion, so the return-ABI cutover cannot silently flatten a new notification error through an inferred closure.

Forcing-function control: a simulated Display-capable error-type change at the real main writer boundary (std::io::Error used as the alternate nominal type) rejects the explicitly typed conversion with matched E0631 / closure-argument mismatch at exit101. Removing only that annotation lets the same changed-error boundary compile at exit0. This is a compiler mechanism probe, not adoption or execution of the provisional Store ABI; no branch cherry-pick occurred. Source was restored after both directions.

Refusal timing audit: the previous eager truthy non-string ID refusal masked a later null/rejection and skipped later serial HTTP batches. Ten added actual-source cases cover normal/algo placement, same-response and cross-request errors, and successful later requests followed by final chosen refusal. The corpus is now 118; named error and request-count assertions pin both effects. Before repair, the live Rust gate failed placement response body 108 (typed ID diagnostic versus the source null ordId error). The TypeScript eager-ID mutation must fail the named deferred-refusal source later-error assertion. This does not widen the string-ID contract.

Two temporary Rust mutations compiled and then failed at 101: eager per-row refusal hit placement response body 108; per-batch refusal hit deferred refusal request count/order 110. The latter retained processing within each response but skipped the next HTTP request, discriminating operation-wide from batch-wide deferral. An initial eager control failed compilation on unused-mut and was rejected as evidence; the corrected control compiled before reaching its named assertion.

Public subscription slice: 35 actual-source cases execute the original subscribe body with installed lodash flatten, comparing exact JSON bytes and frame order/count against real Rust BaseWebSocket sends. Sizes 0/1/99/100/101/199/200/201/1501 cross boundaries; failed send at 0/1 stops later frames. Numeric own keys precede insertion-ordered non-index keys; nested arrays flatten only one level. Scalar/null values and Unicode remain visible. A source 99-size mutation fails named batch-count assertions, and missing lodash fails MODULE_NOT_FOUND. The own-property enumeration is immune to inherited-member reads. The notification audit follows the outgoing no-socket reconnect path to health publication; no disconnected throwing-listener parity is claimed.

The Rust 100-to-99 batch mutation compiled and failed at101 on subscription wire/order/count; the source mutation failed at1 on subscription batch count.

Review found the source flattened topic objects remain live references: a first-send mutation changes the second frame. The source alias witness now pins before/after topics and wire; Rust pins the unchanged later snapshot. This is CHOSEN owned-snapshot-vs-live-alias, not covered by the JSON/UTF8 choice. A double-escaped lone-surrogate witness independently pins source wire acceptance against Rust UTF8 ingress refusal; ordinary astral Unicode/BOM are positive cases.

Older history evaluation-order repair: the original 1178-case oracle compared values and errors but not injected toBase effects. Rust parsed uTime before amount/filled, making zero conversion calls where source made two before the same TypeError. Fourteen added probes across seven fields and two throwing forms now compare callback order/count/arguments and exact errors (1192 total cases). posSide coercion precedes type/status, and amount/filled/reduceOnly precede updatedTime. A source early-uTime mutation fails at1 on source history conversion before coercion error; the compiling Rust early-uTime mutation fails at101 on history conversion effects before uTime error. Existing prototype-output FINDINGs remain unresolved; this repair adds no parity claim.

Heritage-census audit: no TypeScript InterfaceDeclaration member census exists in the OKX oracle family. The single ticker-field census requires a direct TypeLiteralNode; named references, including an injected interface extending ExtraNumeric with required hiddenNumeric:number, fail closed before counting. Its permanent source mutation must exit1 on ticker census requires direct type literal. Other member reads select explicitly named class method bodies and do not claim inherited-field completeness. The cancellation creator discovery parses Rust syn, not TypeScript interfaces. This audit is scoped to our own oracle/discovery files.

History TableHit adoption: 1220 actual-source mapper cases, with source typeof/prototype identity captured before serialization. Rust inherited constructor/__proto__ history outputs now refuse the whole record as CHOSEN closed-typed-output-boundary; source records remain usable with omitted callable fields or present {} prototype fields. The refusal occurs after all source evaluation/conversion work. Eight mixed inherited-plus-later-throw cases compare exact TypeError and callback effects; twenty fallback cases prove lowercase keys exclude the other ten mixed-case prototype names. Earlier 1192 cases and effect controls remain. This is adapted-mapper adoption, not whole-exchange history fetch parity.

History compiling controls: moving refusal before conversions fails at101 on exact later error name (RepresentationError versus source TypeError); reinstating inherited Limit/Closed fallbacks compiles but fails the closed-boundary refusal expectation. Neither a wrong fallback nor eager refusal is accepted.

Earlier-row native numeric audit: a separate 140-case/5-helper maintained-source corpus captures bits and optional presence before JSON serialization. Income/history/closed toNumber, history toOptionalNumber and journal num agree on ordinary, negative-zero, BOM/NEL, prefix, overflow and malformed data probes. Zero implementation defects measured in these cases. A compiling zero-erasure change in income to_number fails at101 on helper native bit identity for string -0; restored code passes. One Node invocation returns the whole corpus. This supplements, rather than rewrites or retroactively certifies, the old value/JSON corpus.

Public ping: twenty actual-source cases compare clock/send effect order, disposal
and exact send errors through core socket sends. Later pong latency observes the
stored timestamp after success and send rejection. Source send-first mutation
fails at exit 1; compiling Rust send-first fails at exit 101 on `ping clock before
send effects`, then restored source order passes. The separate actual base send
and exchange log witness throws a listener TypeError on a disconnected heartbeat;
The old Rust success opposite view flips after the landed WsParent::log cutover:
real core send now propagates the original listener error and Store-local IDs,
without wire or dial. Clock
captures contain finite inputs only and record architecture plus Node major.

Notification cutover consumer: seven production sites propagate the landed ABI.
Five ticker writes use explicitly typed StoreMutationError into the shared
ExchangeError conversion; both incoming health calls propagate their Result.
Real DefaultStore tests retain an existing inner ID plus the same Store's public
notification subscriber ID, original cause metadata, commit-before-failure and
first-failure stopping. A compiling consumer mutation clearing the retained IDs
fails at exit 101 on `notification subscriber identities inner to outer ticker`;
restored propagation passes. Subscriber IDs are Store-local, never global.

After the notification cutover, a compiling ping consumer mutation projecting
errors to message alone fails at exit 101 on `skipped heartbeat retains listener
error and subscriber identities`; restored propagation passes. This proves the
new live witness discriminates current behavior, not historical immunity of any
pre-sweep corpus. Precision consumers remain absent; their census is a negative,
not an already-covered or swap-proof result.

Pretty-symbol supplement: 655 maintained-source cases compare native optional presence, distinct base/quote strings and exact errors. Original market-policy corpus is unchanged; this new oracle feeds only this crate and runs one Node process for the whole corpus. The source swap exits 1; compiling Rust swaps exit 101 on `pretty-symbol native base quote identity 0`, and eager argument coercion exits 101 on `pretty-symbol cold branch must not coerce 390`. Valid JSON lone-surrogate acceptance and Rust UTF-8 refusal have separate escaped witnesses; this is a representation choice, not invalid JSON. No numeric bit capture is used by this string-output oracle.

Cardinality audit found the ticker numeric loop could compare zero fields while209 live cases and write/trace counts passed. CARDINALITY_AUDIT.md records the original byte-identical artifact, independent per-handler fields and216/792 writes/comparisons, permanent empty/partial/count-bypass controls, and the limits of the three collection-return mutation proofs.

## String-selected assertions and filtered witness execution

Whole-suite code census, independently reviewed: five uncounted string-keyed conditional assertions plus two filtered every sites with no nonempty floor. Four affected targets restored at 12/0/0 including their live oracles. Six source controls exit 1 at POSITION-FILTER constructor/prototype executions, TICKER-BRANCH assertion executions (each of the three paths disabled separately), and REST-BRANCH inherited status executions. A Rust disabled-pong conditional compiles and exits 101 at PONG-BRANCH assertion executions. These are injected test-path controls, not production defects; each restores original bytes. No manifest row moved for this repair.

## Pure order classification and priority utilities

The maintained TypeScript AST supplies isAlgoOrder and getAlgoOrderPlacementPriority bodies and the original shared OrderType enum. Six fixed enum cases compare native booleans and native priorities independently; source inventory, native field identity and twelve executed comparisons are asserted. This is the declared closed input domain only; caller partition, sorting and placement are not ported by these helpers. The source capture records darwin/arm64/Node-major provenance and feeds only this crate.

Two production controls independently change StopLoss predicate membership and StopLoss priority. Both compile and exit 101 at ORDER-POLICY native algo identity 3 and ORDER-POLICY native priority identity 3 respectively; source is restored byte-exact by tests/order_policy_controls.py. The native test uses fixed 0..6 bounds independent of the oracle collection and asserts its six-element inventory first.

Restored order_policy target: 1 passed / 0 failed / 0 ignored with live source. Scoped lib/new-test host clippy and wasm32 clippy each exit 0, separately; scoped rustfmt check exits 0. These measurements cover the utility slice, not a full crate or workspace readiness sheet.

## Family construction and exchange lifecycle: ten pending rows

Fresh branch `feat/rust-okx-family-lifecycle` starts at integration
`372972fbfaa126366d6309f4ba15fa84d07bc7a5`. Implementation commit:
`01a806c266ce8a5ef98370f856643a31defd4615`; completed manifest-gate input commit:
`ce7e3679477b2a1662f619086ee29a4e0f2d736a`.

Independent Python tomllib counts over twelve OKX manifests: 113 ported / 19
divergent / 31 pending before, **119 ported / 23 divergent / 21 pending** after.
Exactly **six pending->ported and four pending->divergent**, ten total. All ten
rows name existing indexed Rust targets in `src/family_lifecycle.rs`:

| TypeScript row | Rust target | Status |
| --- | --- | --- |
| OKXExchange | ExchangeState | divergent |
| OKXExchange.constructor | new_exchange | ported |
| OKXExchange.start | start | ported |
| OKXExchange.tick | tick | ported |
| OKXEUExchange | EuExchangeState | divergent |
| OKXEUExchange.fetchAccountMarketOverlay | fetch_eu_account_market_overlay | ported |
| OKXDemoExchange | DemoExchangeState | divergent |
| OKXDemoExchange.constructor | new_demo_exchange | ported |
| OKXEUDemoExchange | EuDemoExchangeState | divergent |
| OKXEUDemoExchange.constructor | new_eu_demo_exchange | ported |

Each class row has its own CHOSEN note: explicit Rust state composition has no JS
mutable callable prototype or reassignable arrow-method properties. The maintained
source class witnesses observe actual own arrow functions, callable prototype
methods, and demo/EU prototype inheritance. This is a class representation choice,
not an invented difference in the method bodies. No KUCOIN row or source changed.

The six methods retain native opaque values, UTF-16 strings, original receivers,
captured callees, original rejection identity, promise continuations and retained
callbacks. Host contracts implement JS expression intrinsics, Map construction,
object spread/own data-property definition, native promise scheduling and class
field/inherited/API/socket assembly. This is authored function-body parity under
those contracts, not a concrete executable JavaScript bridge. The unresolved
createAPI response-provenance/signing/retry row remains pending; constructor
factory forwarding does not certify API completion. No shared production trait or
type was changed.

`tests/family_lifecycle.rs` has five focused native tests, with independently fixed
fixture counts: 9 global construction, 3 global demo, 3 EU demo, 20 startup, 8 tick,
and 3 EU overlay, **46 maintained-source comparisons**. Native assertions precede
source comparison and packing: region and API-factory/parent identities, original
options, forced testnet, demo flags, ordered startup effects and store aliases,
public-only capture, disposal/externally supplied data branches, tick fallback
arrays, negative-zero interval and current-member callback resolution, overlay
ordering/duplicate selection/original limits identity and later live account-row
changes. Errors retain original Rc identity, name, message, code and IDs. The
source oracle executes the maintained AST member bodies and constructors; its
full-class construction witness removes only the export modifier for VM execution.
It supplies host dependencies and does not invoke network or API factories.

Gates actually run:

| Input SHA | Command | Exit / result |
| --- | --- | --- |
| 01a806c266ce8a5ef98370f856643a31defd4615 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| ce7e3679477b2a1662f619086ee29a4e0f2d736a | CARGO_TARGET_DIR=$PWD/sdk-rs/target cargo run --manifest-path sdk-rs/safe-cex-venue-okx/tests/manifest-gate/Cargo.toml --jobs 2 | 0; 166 rows / 168 public items / zero errors |
| ce7e3679477b2a1662f619086ee29a4e0f2d736a | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols / 512 sources; would update 0, add 0, orphan 0 |
| ce7e3679477b2a1662f619086ee29a4e0f2d736a plus control patches, then byte-restored | python3 sdk-rs/safe-cex-venue-okx/tests/family_lifecycle_controls.py | 0; compiled broken target 101, restored target 0 / five passed |

Cargo is `/opt/homebrew/opt/rustup/bin/cargo`; source gates use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The actual extractor WRITER also ran after the manifest edits: exit 0, updated
four manifests with zero added/orphaned symbols. Those writer inputs are retained
in the implementation SHA. Scoped compilation inputs remained byte-identical
between the implementation and completed gate commit; only the nested manifest
checker file changed. The actual checker uses rust_index/check, the twelve venue
manifests and the existing three-row OKX journal manifest, plus only existing OKX
exemptions and one exact new LifecycleOps plumbing exemption. It does not exempt
missing behavior or replace the Rust item index with a fabricated inventory.

Five independent production families were mutated together; every native test
failed at its own assertion, after compilation, at exit 101. Restoring exact source
bytes made all five tests and live source comparisons pass at exit 0:

| Family | Production mutation | Named decoded-native failure |
| --- | --- | --- |
| constructor / global class state | EU region becomes global | CONSTRUCTOR native region selection |
| demo constructors / class flags | force testnet=false | DEMO native forced testnet |
| startup | private connection becomes business connection | START native ordered load and connection effects |
| tick | retained callback resolves log instead of current tick | TICK native retained callback resolves current member |
| EU overlay | copied limits become account ID | OVERLAY native original limits identity |

Zero survivors. An earlier development restoration exposed an export-modifier
error in the new full-class VM witness; it was corrected before the recorded
committed controls and passing restoration. Earlier scoped-checker path issues
were corrected before its accepted zero-error result. Neither is counted as a
successful gate or production mutation proof.

Merge-down: fetched integration `f364cee096ff92584c157cad8b96f12b92cf3a09` merged
cleanly at `8c2dc091778c8e27efbc3afc0eb85bd22b90bb6b`. Standalone
`git diff --exit-code ce7e3679477b2a1662f619086ee29a4e0f2d736a HEAD --
sdk-rs/safe-cex-venue-okx/src sdk-rs/safe-cex-venue-okx/tests` returned 0. Separate
byte proofs over the whole venue, OKX source and manifests, journal manifest,
source loader, shared types/num/store/exchange/transport/crypto/parity crates,
SDK Cargo manifest/lock, extractor, TS types/safe-math, package.json and yarn.lock
also returned 0. Gate exits stand on unchanged inputs. Parsed exemption union
retains all 1257 incoming entries in original order and exactly one owned addition.
Fresh merge-head tomllib again measured 119/23/21, exactly six/four transitions
against the actual integration parent. This receipt changes documentation only.

Not run: full venue/workspace test suites, workspace compilation, clippy, wasm,
live exchange/network tests, concrete runtime class/API assembly, PR creation or
merge. Scoped Rust formatting and git diff --check were run. Generated logs and
the nested tool lockfile stay ignored under target/ or the tool's .gitignore;
this manually authored SHA-bound sheet is tracked and committed.

Final delivery supplement: `e5e026a0f27491962621546b2de3d8d2c2ed4d95` strengthens
the source constructor witness to retain the actual Base receiver before socket
construction and assert each socket's parent against that retained reference.
The refreshed family_lifecycle target at that SHA returned 0: five passed, zero
failed, all 46 maintained-source fixtures. Rust production and native test bytes
are unchanged from the control measurements; only the source-oracle parent
identity assertion was strengthened. Earlier control exits remain bound to their
actual ce7e367947 input SHA rather than being relabeled.

A second fetch merged integration `f17498f6e1e8b8e8b93c75bc11f2535b2b4bf8eb` at
`76f2f6022d9d2b4760b29416f1b385dee249f30b`. The sole conflict was appended
parity exemptions: resolution preserves every incoming record plus exactly our
LifecycleOps record. Parsed union proof returned 0: 1267 incoming records plus one
owned record; the OKX-filtered checker exemption records are identical to ce7e367947.
Standalone source/tests byte proof against e5e026a0f2 returned 0, as did the separate
shared build/oracle-input proof listed above. No relevant shared code or compile
input changed. Fresh tomllib against this actual integration parent again returns
119 ported / 23 divergent / 21 pending, six pending->ported and four
pending->divergent. The final receipt adds documentation only; the refreshed test
and earlier unchanged compilation/indexer inputs retain their recorded exits.

## Websocket account events and state: eight pending rows

Branch `feat/rust-okx-websocket-account-events` begins at
`8a98237aa1ea496db3b0e16bafedcce0bfd06fc4`. Reviewed implementation and control
input SHA: `32d1b35b42df98795f4a21e770273ecd777e4446`.
Independent tomllib over twelve OKX manifests measures 119/23/21 before and
**124 ported / 26 divergent / 13 pending** after: exactly **five
pending->ported and three pending->divergent**, eight rows total.

All targets are in `safe-cex-venue-okx/src/websocket_account_events.rs`:

| TypeScript row | Rust target | Status |
| --- | --- | --- |
| OKXPrivateWebsocket | PrivateSocketState + PrivateSocketState::initialize | divergent |
| OKXPrivateWebsocket.handleOrderTopic | handle_order_topic | ported |
| OKXPrivateWebsocket#isDuplicateFillEvent | is_duplicate_fill_event | ported |
| OKXPrivateWebsocket.handlePositionTopic | handle_position_topic | ported |
| OKXPrivateWebsocket.handleAccountTopic | handle_account_topic | ported |
| OKXPublicWebsocket | PublicSocketState + PublicSocketState::initialize | divergent |
| OKXPublicWebsocket#onMarketsChanged | on_markets_changed | ported |
| OKXBusinessWebsocket | BusinessSocketState + BusinessSocketState::initialize | divergent |

Every class row has its own CHOSEN note: Rust owns explicit data/callback state
instead of exposing JS prototype inheritance/getters and replaceable arrow-method
instance properties. Original source class initialization, own onMessage functions,
private/business endpoint getters, public Base prototype inheritance, and business
private-prototype inheritance/resync getter are witnessed. The native public
handlers preserve their original payload identity and resolve current methods at
invocation, including replacement after initialization. Three class rows do not
claim a concrete JS class bridge. No KUCOIN implementation or manifest was authored.

Method contracts retain opaque native values, original references, UTF-16 and
binary64 carriers, original errors and captured receiver-bound callees. Host
capabilities own JS expression/property/Map/iterator/spread semantics, live helper
bindings, JSON encoding and concrete class/socket assembly. This maps authored
function bodies, not the pending mapOrders/mapPositions methods or a concrete
socket runtime. The native stringify fixture retains an opaque original packet;
source actually encodes/decodes JSON. No Rust encoder parity is claimed.

Seven focused tests independently enumerate **56 source fixtures**: four
private/business initialization, two public initialization, seventeen order events,
eight deduplication, seven position events, ten account events and eight market
changes. Each fixed family floor is asserted independently of the oracle output.
Native values/effects/aliases are asserted before source observation and packing.
The maintained AST supplies original event bodies and full socket classes; actual
roundUSD, safe-math/subtract, isNotNumber, lodash sumBy and OrderStatus are loaded.
The test loader explicitly resolves the monorepo math alias to its actual source.
An initial missing-alias loader failure and a mistaken public endpoint-descriptor
witness were corrected before accepted gates; they are not mutation evidence.

Native assertions cover terminal status/order before fill, mapped-row/ID/market
identity, converted amount/parsed price, strict TTL cutoff without duplicate
refresh, UTF-16 lone-surrogate keys, unused price-coercion failure, rounded used/PnL,
balance reread after position-write reentry, finite account filtering/overflow,
retained balance fields, 100-topic batches and index identity. The first send
mutates a topic in the later frame; a decoded native assertion proves that later
frame sees the original topic alias. Original rejection Rc identity, name, message,
code and IDs are pinned. JSON convergence is not used as a native bit assertion.

Gates at the reviewed input SHA:

| Command | Exit / observed result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| CARGO_TARGET_DIR=$PWD/sdk-rs/target cargo run --manifest-path sdk-rs/safe-cex-venue-okx/tests/manifest-gate/Cargo.toml --jobs 2 | 0; 166 rows, 180 public items, zero errors |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols / 512 sources, would update/add/orphan zero |
| python3 sdk-rs/safe-cex-venue-okx/tests/websocket_account_controls.py | 0; compiled broken target 101, byte-restored target 0; seven tests pass |

Actual extractor WRITER after manifest edits returned 0: initially updated three
manifests, no symbols added/orphaned; the final note correction was followed by an
actual writer run updating zero manifests. Cargo is
`/opt/homebrew/opt/rustup/bin/cargo`, jobs 2; source gates set
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The checker uses actual rust_index/check, venue plus existing journal manifests,
and existing OKX exemptions plus exactly one SocketEventOps plumbing exemption.
No missing behavior or fabricated Rust inventory is exempted.

Seven independent production controls compile together and each fail its own
native test at exit 101. Restoring exact source bytes returns 0, seven passed and
all 56 live source comparisons. Private/business initialization shares one body.

| Family | Mutation | Named decoded-native failure |
| --- | --- | --- |
| private/business state | 60000ms becomes 60001ms | CLASS native private dedupe window |
| public state | ticker callback dispatches mark-price handler | CLASS native handler identity and late member lookup |
| orders | filled terminal status becomes Canceled | ORDER native terminal status |
| dedupe | prune seenAt<=cutoff instead of < | DEDUPE native retains exact cutoff |
| positions | exchange used/upnl fields | POSITION native rounded margin USD |
| accounts | exchange free/total fields | ACCOUNT native free after used |
| market changes | 100-element batches become 99 | MARKETS native 100-topic batch widths |

Zero survivors. Controls remain attributed to their actual implementation SHA.

Merge-down fetched integration `9c663a908f40c2955e27b1ce58f97fb5c08fb331` and
merged cleanly at `27876185db76356f8d97d6c20882ca3a5b0d23bb`.
Standalone `git diff --exit-code 32d1b35b42df98795f4a21e770273ecd777e4446 HEAD --
sdk-rs/safe-cex-venue-okx/src sdk-rs/safe-cex-venue-okx/tests` returned 0.
Owned manifests, journal manifest, source bodies/loaders/helpers, types/num/store/
transport/crypto/parity crates, SDK Cargo manifest, extractor and yarn.lock also
have a separate byte proof at exit 0. The shared exchange websocket file changed
with incoming before-close support, so all-build-input preservation is NOT claimed.
Scoped compilation and the focused seven-test/56-fixture target were rerun at the
actual merge SHA: both exit 0. Fresh extractor check at that SHA also exits 0,
7341/512 and zero updates/additions/orphans.

Independent parsed comparisons show Cargo.lock changed only the bitrue package
record, package.json only the KUCOIN oracle script, and the OKX-filtered checker
exemptions stayed identical. Parsed union retains all 1269 incoming exemption
records in original order plus our one unchanged SocketEventOps record. Fresh
merge-head tomllib against the actual integration parent measures 124/26/13 and
exactly five/three transitions. The gate receipt itself adds documentation only.

Not run: full venue/workspace suites, workspace compilation, clippy/wasm, live
exchange tests, concrete class/transport bridge, PR creation or merge. Scoped
formatting and git diff --check were run. Generated gate logs remain ignored under
target/; this manually authored SHA-bound gate sheet is tracked and committed.

The actual scoped manifest checker was also refreshed at
`27876185db76356f8d97d6c20882ca3a5b0d23bb`: exit 0, 166 rows / 180 public items /
zero errors. The prior gate archive remains byte-exact as this sheet's prefix.

Final pre-push fetch merged integration `bcba22af177e3c42704feb058ef7539d58ddcd74`
at `2748fa5e6695621cd17010a8147fdbca3be5fa6a`. The append conflict in exempt.toml
was resolved by retaining all 1376 incoming records and our one exact plumbing
record; the OKX-filtered exemptions remain identical. Standalone source/tests
and separate shared build/source/lock/package input byte proofs against the
refreshed gate SHA 27876185db both return 0. Gate inputs remain retained. Fresh
final-parent tomllib measures 124/26/13 and exactly five pending->ported plus
three pending->divergent. This supplement changes documentation only.

## Order and execution composition: eight pending rows, all ported

Branch `feat/rust-okx-order-execution-composition` starts at integration
`7daf9d4c8c6805aaff3aaee1fbc6a0bbe6eedab2`. Reviewed implementation and gate
input SHA: `7a7875f391e36ab95896a687690be742c376dd2c`.
Independent tomllib over twelve OKX manifests measures 124/26/13 before and
**132 ported / 26 divergent / 5 pending** after: **eight pending->ported,
zero pending->divergent**. Every moved row has a real indexed target in
`safe-cex-venue-okx/src/order_composition.rs`:

| TypeScript row | Rust target |
| --- | --- |
| OKXExchange.placeOrder | place_order |
| OKXExchange.placeOrders | place_orders |
| OKXExchange.mapOrders | map_orders |
| OKXExchange.mapPositions | map_positions |
| OKXExchange#formatAlgoOrder | format_algo_order |
| OKXExchange#formatCreateOrder | format_create_order |
| OKXExchange#getReduceOnlyTriggerOrderType | get_reduce_only_trigger_order_type |
| OKXExchange.fetchHistoricalFills | fetch_historical_fills |

The Rust bodies own branch/control flow, original value construction, callbacks,
ordered helper invocations and awaits. Explicit host contracts supply opaque native
values, captured receiver/member calls, original errors, JS properties/tables and
prototype values, arrays/iteration/sorting, primitive intrinsics, existing helpers,
promises and BigNumber constructors/methods. Native values are not projected through
JSON. Inherited constructor table results retain callable values and the same
original function handle across type/status/side; source and native identity are
checked before packing. Original info and market/option aliases also remain live.
Concrete class/XHR assembly, caller-ID policy/helper implementations, typed core
Order/Position ABI and a production BigNumber runtime are not claimed by these
function-body mappings. The API factory remains pending with its existing note.

Eight focused tests have independently fixed floors totaling **89 maintained-source
fixtures**: single2, placement9, trigger8, algo11, creation16, order mapping16,
position mapping12 and historical fills15. Source AST bodies execute with actual
OKX constants/enums, safe-math, inverseObj, omitUndefined, createPositionId,
normalizeExecutionPositionSide, uuid/nanoid host, lodash partition and bignumber.js.
Underlying caller/helper/HTTP methods are controlled live dependencies. No network
request occurs. Native decimal fixture arithmetic uses exact coefficient/scale
integers for its bounded finite decimal profile; it does not claim a complete
BigNumber implementation or approximate lots with binary64 in production.

Native assertions precede source observation: original singleton options and
awaited output, preflight before mode and normal-before-algo batches, S/T/C priority,
opening trigger classification and adjusted fields, broker/client IDs and exact
10/10/0.3 lot sizes, normal filled/remaining conversions and negative-zero clock,
inherited callable type/status/side identity, original info aliases, position base
contracts/notional/leverage, and distinct trade-ID fills with later duplicate
quantity replacement/newest-first order. Request/branch/error fixtures additionally
cover early guards, empty/malformed data, zero-size diagnostic, missing markets,
caller-split rejection, partial lot cap, zero/NaN positions, trigger/oco/trailing
mapping, fallback identifiers, time windows, limit clamp, and original helper/XHR
errors. Original rejected Rc/name/message/code/IDs and fresh constructed errors are
kept distinct. This floor is not exhaustive coercion/iterator/GC/async coverage.

Gates at the reviewed implementation SHA:

| Command | Exit / observed result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| CARGO_TARGET_DIR=$PWD/sdk-rs/target cargo run --manifest-path sdk-rs/safe-cex-venue-okx/tests/manifest-gate/Cargo.toml --jobs 2 | 0; 166 rows / 189 public items / zero errors |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols / 512 sources; zero updates/additions/orphans |
| python3 sdk-rs/safe-cex-venue-okx/tests/order_composition_controls.py | 0; compiled broken target101, byte-restored target0, eight passed / zero failed |

Actual extractor WRITER after manifest edits also returned0: updated one manifest,
zero added/orphaned symbols. Cargo is `/opt/homebrew/opt/rustup/bin/cargo`, jobs2;
source gates use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The actual rust_index/check gate includes twelve venue manifests plus the existing
three-row OKX journal manifest. The sole new exemption is CompositionOps plumbing;
no behavior, missing target or fabricated index is exempted. Initial development
compiler/import/literal errors were corrected before these accepted gates and are
not counted as mutation proofs.

Eight independent production family controls compile together; all eight tests
fail at their own decoded-native assertion at exit101. Exact source restoration
returns0, eight tests passed and all89 live source fixtures pass. Zero survivors.

| Family | Mutation | Named native failure |
| --- | --- | --- |
| single placement | discard awaited result and return empty | SINGLE native result count |
| batch placement | reverse priority subtraction | PLACE native algorithm priority order |
| trigger classification | take-profit branch becomes stop-loss | TRIGGER native buy below reference type |
| algo formatting | triggerPx uses limit price | ALGO native adjusted trigger price |
| creation formatting | omit decimal amount-step multiplication | CREATE native decimal split widths |
| order mapping | exchange filled and remaining fields | ORDERS native converted remaining |
| position mapping | exchange notional and base contracts | POSITIONS native base contracts |
| historical fills | use billId where tradeId is present | FILLS native newest first |

Merge-down fetched integration `1f7e8851f837a12a309df28d0e3aedade840b44c`, merged
at `9371b14d19f8d6799f0ec6a805d191f164fde9d1`. The append conflict in exempt.toml
was resolved by preserving all1413 incoming entries in their original order plus
our one exact CompositionOps record. The OKX-filtered checker exemptions are
identical to the gate input. Fresh tomllib against this actual integration parent
again measures132/26/5 and exactly eight pending->ported transitions.

Standalone `git diff --exit-code 7a7875f391e36ab95896a687690be742c376dd2c HEAD --
sdk-rs/safe-cex-venue-okx/src sdk-rs/safe-cex-venue-okx/tests` returned0. A separate
byte proof over shared types/num/store/exchange/transport/crypto/parity, SDK Cargo
manifest/lock, venue Cargo manifest, extractor, owned/journal manifests, original
OKX/types/utils/math-helper source, source loader and yarn.lock also returned0.
package.json differs only in the incoming KUCOIN oracle script; parsed metadata and
all other scripts are identical. No authored KUCOIN changes. Gate inputs survive
merge; the recorded exits remain bound to their actual SHA. This receipt adds docs.

Not run: full venue/workspace suites, workspace compilation, clippy/wasm, live
exchange/API tests, full runtime/BigNumber/caller-policy implementation, PR creation
or merge. Scoped formatting and git diff --check were run. Generated control logs
remain ignored under target/; this manually authored SHA-bound sheet is tracked.

Final pre-push fetch merged integration `627f8ad7aa6a5a52d547f456ed8e30decb4945c9`
at `0896cd60cddce052458efc547ecea33450b80119`, cleanly. Standalone owned source/
tests preservation against 7a7875f391 and the separate shared build/source/lock/
package preservation against 9371b14d19 both return 0. Final-parent tomllib again
measures 132/26/5 and exactly eight pending->ported transitions. Parsed exemption
union preserves every incoming record plus our exact one; OKX-filtered checker
inputs remain identical. The original gate archive is retained as this sheet's
byte-exact prefix. This delivery supplement changes documentation only.


## History reads, stream subscriptions and API factory: five remaining rows

Reviewed and gated SHA: `e75a8808b6c824d0bea6c437da5e279d55431fea`.
Fresh branch base: `65cdf874589742c3ce300e77c5081a1f9242c6b5`.
Implementation: `4a76dad4c631811ab0fae7b6ecdb0fab78ee4b6f`.
The first merge-down retained both exemption lanes at `91fe63289e55b2ab9ce563e84bfdaed595eb9aa7`.
PR 7812 recovery merged integration `661cd4f26ca6aa8ae09684e52f76897afe1cb456`
at `286b6731a0008ad77644e42a66fc7e03e123d263`, retained all incoming exemptions
plus our RuntimeOps record, and pushed before gates. A control-only correction
was then pushed at the reviewed SHA before the accepted gates below.

Tomllib over the twelve OKX venue manifests measures **137 ported / 26 divergent /
0 pending**: exactly **five pending->ported**, zero new divergent, against the
actual 661cd4f26c integration parent. Each transitioned row has a checked real
Rust file and public function indexed by the actual parity checker.

| Behavior group | TypeScript row | Rust function in src/history_stream_api.rs |
| --- | --- | --- |
| History reads | OKXExchange.fetchIncomeHistory | fetch_income_history |
| History reads | OKXExchange.fetchOHLCV | fetch_ohlcv |
| Stream subscriptions | OKXBusinessWebsocket.listenOHLCV | listen_ohlcv |
| Stream subscriptions | OKXPublicWebsocket.listenOrderBook | listen_order_book |
| API composition | createAPI | create_api |

The Rust bodies preserve bill-based limits, cursor/stall/sleep/cap behavior and
last-write dedupe; candle date/endpoint/default/sort/column behavior; connection
polling, cancellation, shared subscriber cleanup, order-book filtering/totals
and persistent callback aliases; factory guards, retry policy, interceptors,
signing and tunnel installation. RuntimeOps uses the existing opaque value,
iterator, callable and await capabilities and adds callback construction only.
Its exact plumbing exemption adds no behavior exemption or fake target.

The previous API note's response-provenance FINDING is repaired in shared core
on the supplied integration head. RestResponseBody now retains encoded bytes,
native adapter values and transform outputs. The additional native phase gate
uses that actual carrier and shared bigInt parser, then the caller-selected
response transform; all fourteen direct/HOW observations match the maintained
factory, actual Axios and actual production HOW adapter. A direct quoted scalar
stays a string while HOW's second parse promotes it; numeric bits are asserted
before serialization. The factory keeps the original opaque XHR and dependency
carriers, rather than trying to infer their phase from bytes.

The focused target has six tests and **93 maintained-source fixtures**: seventeen
income, twelve REST candle, twelve candle subscription, fourteen order-book
subscription, twenty-four factory composition and fourteen response-phase
observations. Fixed decoded-native assertions run before source comparison.
They cover newest/ascending ordering, duplicate replacement, bill limits and cap
warning, column-seven volumes, contract multiplication/totals, book aliases,
shared cleanup, signed headers/HMAC, request/client aliases and response types.
The source oracle extracts the maintained bodies rather than porting a second
reference implementation. Clock, timer, XHR, mapper and callable capabilities
are controlled host fixtures; date/library and host/runtime behavior outside
this corpus are not exhaustively established by these gates.

Accepted gates at the reviewed SHA (Cargo jobs 2; no full suite):

| Gate / command | Exit and result |
| --- | --- |
| `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2` | 0 |
| `/opt/homebrew/bin/python3.11 sdk-rs/safe-cex-venue-okx/tests/history_stream_api_controls.py` | 0; mutated target 101 with five named native failures, byte-restored target 0 with six passed / zero failed |
| Focused cargo target invoked by that script: `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --test history_stream_api --jobs 2 -- --nocapture --test-threads=1` | restored 0; all 93 source observations pass |
| `CARGO_TARGET_DIR=$PWD/sdk-rs/target cargo run --manifest-path sdk-rs/safe-cex-venue-okx/tests/manifest-gate/Cargo.toml --jobs 2` | 0; 166 rows / 195 public items / zero errors |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; zero updates/additions/orphans |
| Scoped rustfmt `--check` and `git diff --check` | 0 |

Source commands use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The actual extractor writer returned 0 before merge-down: zero manifest changes,
zero additions/orphans. The checker includes the existing three-row journal
manifest, using the actual rust_index/check implementation and OKX-filtered
exemptions. Owned source/tests/manifests survived both merge-downs byte-for-byte;
the reviewed follow-up changes only the control script. Controls restore the
exact production bytes in finally, and git diff --exit-code then returns 0.

| Family | Production mutation | Named native assertion reached |
| --- | --- | --- |
| Bill history | reverse newest-first comparator | INCOME native newest record first |
| REST candles | reverse ascending comparator | CANDLES native ascending timestamps |
| Candle subscription | change volume index seven to five | SUBCANDLE native volume column seven |
| Order-book subscription | multiply contract precision by ten | BOOK native contract multiplier |
| API factory | use secret for access key | FACTORY native signed API key |

The batch compiles, all five controls fail at exit 101, and the independent
response-phase test still passes. Restoration returns 0 with all six tests
passing. The initial book mutation removed the only use of its parameter and
was rejected as a compiler failure; it was corrected before this accepted
proof and is not counted as a discriminating control. Earlier development
compiler/oracle errors are also not mutation evidence.

Not run: full venue/workspace suites, clippy, wasm builds, live exchange/API
traffic, browser checks, credential checks, deployment or PR creation/merge.
Generated logs remain ignored under target/; this manually authored gate sheet
is tracked. This receipt changes documentation only and retains the archive
above as a byte-exact prefix.

## Private literal-wire owner repair

The compiler census independently exposes `types::RegionWire` and
`types::InstrumentWire`. Their former derived decoders accepted tagged-null maps
although the maintained TypeScript literal unions admit strings. Both owners
remain in the census and now require `String::deserialize` before delegating to
private Serde remote schemas, following
[the shared string-only enum recipe](../safe-cex-parity/accepted-set/MAP-ONLY-INGRESS.md#string-only-literal-enums).
Serialization, native variants and typed `From` mappings remain covered. The
remote schema declarations introduce no independent `Deserialize` owner.

The original native tests failed both named tagged-map assertions before repair.
After repair, all four library tests and both table tests pass, including the
live TypeScript literal inventory. Each literal has native-variant and existing
serialization checks through owned, borrowed, qualified and text entry paths;
each tagged-null form is refused through those paths. Fresh compiler discovery
still reports 145 owners and four aliases. Two selected actual production
accepted-set probes pass with zero failed owners, using the independently
reviewed contracts for these private owners.

Run `python3 safe-cex-venue-okx/tests/private_wire_controls.py` from `sdk-rs`
with the normal live TypeScript oracle setup. The checked-in control separately
removes each actual owner's string frontdoor, requires successful compilation
and its named tagged-map regression failure, then restores exact source bytes
and passes. It also adds `OKX_ALIAS` to the actual remote schema and proves the
complete TypeScript literal inventory fails before exact restoration passes.
Separately executed production-probe controls removed each guard independently:
only that owner's native receipt failed, and both receipts passed after each
restoration. Compiler failures never count as a successful negative control.
This closes these two owner contracts; it does not claim the remaining OKX
compiler inventory is fully classified.

## Production intrinsic spread and UTF-16 repair

The original 53-case order-read suite passed with its private mock dispatcher.
Deleting that table and using the production dispatcher with shared `Js` and
`Thrown` compiled, then failed the unchanged independent order-count assertion
(2 versus 5). The existing websocket corpus, after the same cutover, exposed a
second mismatch: `dedupe/lone` used U+FFFD where the maintained source retains
U+D800. No source fixture or expected result was changed for either repair.

The migrated suites retain 53 order-read and 56 websocket scenarios. The
websocket fixture uses actual shared JSON stringify/parse at its wire boundary;
external callbacks remain scripted. The new production Exchange case loads five
distinct groups, checks their native IDs and order variants in source order,
commits them through Store, then asserts symbol cancellation posts the normal
order and every algo ID. Direct dispatcher tests retain object references,
iterate strings and Sets, materialize holes, preserve negative zero, and reject
a noniterable fifth group. The last check asserts TypeError kind; it makes no
claim about engine-specific diagnostic wording.

Maintained authority: `okx.exchange.ts` fetchOrders at lines 733–745,
`okx.ws-public.ts` onMarketsChanged at lines 90–140, and `okx.ws-private.ts`
isDuplicateFillEvent. These are repaired Rust porting bugs, with no new source
divergence, source change or accepted-contract exemption.

Executed `python3 safe-cex-venue-okx/tests/production_intrinsic_controls.py`
from `sdk-rs`: four independent controls each compiled, failed the named runtime
assertion at exit 101, restored the exact source bytes, and passed at exit 0:

| Restored defect | Actual failure assertion |
| --- | --- |
| Two-group production spread, order-read suite | `ORDER native independent result count` |
| Two-group production spread, production Exchange | `OKX production retains all five order groups in source order` |
| Two-group production spread, new-market subscriptions | `MARKETS native 100-topic batch widths` |
| Lossy UTF-16 colon join, dedupe | `maintained source dedupe/4` |

The focused restored baseline passed 16 production Exchange tests, two direct
dispatcher tests and all seven websocket family tests; the order-read control
also passed its complete 53-case source comparison after restoration. Full scoped
suite, host/WASM lint, formatting and parity results are recorded in the PR.
The other five harness migrations and missing API/history intrinsic names are
separate remaining audit work.

### Terminal Store bridge and production private frames

The original real-binding order witness failed with native status Open instead
of Closed when the JS Store facade lacked `markOrdersTerminal`. The bridge now
calls the existing native terminal-memory operation. Source authority is
`store/store.base.ts:517-529` and `okx/okx.ws-private.ts:229-254`.

Three production-binding tests pass through actual private socket dispatch:
orders/final fills (all filled, canceled and mmp-canceled states), positions and
account balance. They pin terminal commit before fill, the exact bare fill object
and converted amount/price, duplicate suppression, no canceled/account/position
fill, and stale REST snapshot refusal. Three direct native Store/facade tests
pass for absent-ID memory, subscriber error metadata and raw UTF16 ID preservation.
The shared emitter behavior is inherited from #425, not reimplemented here.

The core Store-correctness runner now owns five `terminal-*` compiled controls:
missing facade registration, suppressed terminal-memory writes,
an empty-Store shortcut, discarded subscriber errors and lossy UTF16 ID replacement.
They target the live canonical graph and existing `js_store_terminal` witnesses.
The raw-ID control follows maintained DefaultStore: surrogate IDs remain distinct
from U+FFFD. The original typed-ID refusal expectation is superseded by #1180.
See `../safe-cex-exchange/STORE_CORRECTNESS_REVIEW.md` for exact commands and
the per-control assertion requirements.

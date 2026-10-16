# Worker fixture identity and numeric capture audit

Baseline: `6166be886d`, merged with integration `a3d514bec8` at `114725d26f`.
This audit is in progress. Candidate counts are not repaired-defect counts.

## Inventory method and scope

A TypeScript AST walk examined all 30 baseline source-oracle modules. It found
66 same-object equal-literal groups in 16 files. It does not classify computed
or shorthand initializers as literals. A separate `syn` walk of the 39 tracked
Rust test/support files parsed 51 struct literals and 241 `json!` objects,
including assertion-macro arguments: 73 repeated-initializer groups. Fourteen
are dummy host credentials; eight use collections/defaults/options, sometimes
of different Rust types. Generated task samples and candle emissions were
examined separately. Remaining candidate groups still require disposition.

A tracked-text consumer census outside this crate found three manifest-note
references and no executable consumers of these oracle artifacts. CI executes
this crate's own tests. `task_types_oracle.mjs` feeds the local task and record
acceptance gates; `task_domain_numeric_oracle.mjs` feeds the local numeric graph
gate. Neither was found feeding the shared numeric corpus or another crate.
Their original bytes are nevertheless preserved, with a separately labeled
`task_identity_oracle.mjs` supplement. This supplement makes no retroactive
independence claim for earlier results.

## Identity repairs and test shape

| Gate | Previous assertion shape | Added identity evidence |
| --- | --- | --- |
| `task_types` | Generic deserialize/serialize roundtrip; numeric fields also had direct readers | `task_identity`: six records, 91 source-AST leaf paths, 130 per-field cases through raw, owned, borrowed and outer-envelope decodes; explicit native field projections and exhaustive native enum matches |
| `record_inputs` | Settings and serialized-error positive cases only roundtripped | Native TP/SL enum variants and native message/name/stack assertions |
| `error` | Error helper output was serialized before comparison | Native message/name/stack assertions, including primitive fallbacks |
| `payload` | Envelope, packed delta and operation unions roundtripped | Native envelope presence, packed index/mask/values/update ID; `payload_identity` maps every incoming/outgoing name to native variants and checks full source coverage |
| `subscriptions` | Candle open/high/low/close/volume all used the same number | Distinct OHLCV values constructed as native Candle fields before forwarding; emitted fields compared with actual source |
| `twap_postprocessor` | Two options booleans always true | Independent flag combinations; output flags read from native ActiveTWAP fields |
| `chaser_instance` | Native getters, but pending/fresh always equal in flag transitions | Both mixed flag transitions added to every existing trace combination |
| `postprocessors` margin | Native fields, but rounding made long and total both 1e16 | Three actual-source cases with distinct long/short/total exposures and native bit assertions |
| `worker_log` | Actual output envelopes; override account/exchange repeated | Distinct override strings |

`task_numeric_ingress`, `task_domain_numeric_ingress` and
`record_numeric_ingress` already read individual native numeric fields with
per-field probes. These are identity evidence for those numeric paths, not
for arbitrary string/boolean fields. The enrichment oracle's typed expected
orders use the same decoder as actual inputs; that comparison alone is not
independent field identity. Its separate direct attempts=3/maxAttempts=7
assertions provide identity evidence for those enrichment fields.

Compiling mutations failed at `TASK-IDENTITY native field TwapOptions`,
`TASK-IDENTITY native field ChaserOrderOptions`, `RECORD-IDENTITY native TP
trigger`, `MARGIN-IDENTITY native`, `TWAP-PROJECTION`, `CHASER-IDENTITY native
state`, `SUBSCRIPTION-IDENTITY source callback fields`, and `PAYLOAD-IDENTITY native Init`. Under the margin
swap, the original rounding test still passed while the supplement failed.

## NaN capture audit

Capture provenance: Node 26.7.0, darwin/arm64. These are live oracles, not frozen
cross-architecture goldens. Original task capture provenance is recorded here
and in the supplement without changing those original artifacts.

Instrumentation wrapped `DataView.setFloat64` and `Buffer.writeDoubleBE` while
executing every one of the 30 baseline oracles. All exited zero. Twelve wrote
binary64 captures; no NaN reached a raw writer.

| Corpus | Binary64 writes | NaN handling |
| --- | ---: | --- |
| chaser_price | 2367 | Existing canonical `NaN` token before bit capture |
| ui_leverage | 8148 | Existing canonical `NaN` token |
| twap_helpers | 7238 | Existing canonical `NaN` token |
| twap_lifecycle | 12616 | Existing canonical `NaN` token |
| twap_postprocessor | 59004 | Existing canonical `NaN` token |
| twap_state | 47287 | Existing canonical `NaN` token |
| risk_postprocessor | 1055 | Current corpus finite; added explicit canonical NaN capture for future arithmetic results |
| task_domain_numeric | 10 | Fixed valid JSON number tokens; overflow is infinity, never NaN |
| task_types | 7 | Fixed valid JSON number tokens |
| worker_buffer | 7 | Fixed valid JSON number tokens |
| worker_error | 6 | Fixed valid JSON number tokens |
| worker_log | 5 | Fixed valid JSON number tokens; separate nonfinite witness uses semantic labels |

Risk capture now normalizes only NaN to `7ff8000000000000` on source and Rust
capture boundaries. Signed zero, finite numbers and infinities remain exact.
The capture supplement exercises both NaN signs and multiple payloads, both
zero signs, positive/negative finite values, a subnormal and both infinities.
Compiling controls fail `CAPTURE signed-zero identity Rust` and `CAPTURE finite
and infinity payload identity Rust`. The first finite-blind control was masked
by the signed-zero assertion; it was narrowed to finite nonzero values and
then failed the finite assertion. No masked failure is claimed as proof.

No merge-up readiness claim is made until the remaining fixture dispositions,
restored gates and merged-state adversarial review are complete.

Two additional inline Node captures, in `record_numeric_ingress` and
`task_numeric_ingress`, use the same ten fixed valid JSON number tokens and
cannot produce NaN. Their captures were executed by the scoped worker gate.
The Rust fill/manager comparisons explicitly test `is_nan` before bit identity;
price, TWAP and leverage helpers use the canonical NaN token. JSON-tree
comparators receive JSON numbers or null, not a raw NaN bit pattern. The
store-preparation native bit witness is specifically negative zero.

## Follow-through on generated values and assertion shape

The generated chaser postprocessor cases assigned `maxAttempts=size` in all 90
cases, outside the literal-only census. Swapping those production fields compiled
and the original test passed. Distinct maxAttempts=17.25 and explicit native
ActiveChaser projections replace that blind comparison; optional fields and enum
variants are read without the model serializer. The repaired control fails
`CHASER-PROJECTION native first case`. Order-book callback fixtures also distinguish
amount from cumulative total, constructing native levels before forwarding.

| Gate family | Existing observation and remaining scope |
| --- | --- |
| call_method, handlers, dev, ui_state | Dispatch targets, runtime effects, state and replies; cloned domain payload identity remains to be supplemented |
| chaser_completion, chaser_fill, chaser_health, chaser_manager, chaser_price | Native scalar/getter results, error identity and effects; selected serialized Order payloads remain under audit |
| chaser_instance | Native getters/state; ActiveChaser projection now reads every native field |
| chaser_enrichment | Native required fields plus the separately labeled optional-field supplement below; all Order/extra fields read directly, with exhaustive enum matches |
| displayed_positions, store_preparation | Required domain fields have direct native supplements below; optional domain fields and displayed Position.side remain open |
| message_stats, store_payload | Native constructed stats or individually inspected packed fields and effects |
| subscriptions | Native callback records and posted JSON; distinct Candle and book fields prevent symmetric ingress/egress cancellation |
| risk_postprocessor | Native error/effect observations and native profile numeric bits; shared profile serialization finding remains separate |
| twap_helpers, twap_state, twap_postprocessor | Native numeric readers, flags, scalar/enum projections and the 24-case retained Market supplement below; broader copied-payload audit remains open |
| ui_leverage | Native numeric patch fields |
| worker_buffer, worker_error, worker_log | Callback effects, posted JSON and independent native bits; there is no typed roundtrip between both compared envelopes |
| worker_logging | Value-to-Value helpers; no symmetric model codec |
| throttle | Timer, clock and callback effects, no model codec |
| oracle_dependencies, literal_inventory | Dependency execution and accepted spellings, not model field-identity claims |
| chaser_lifecycle, twap_lifecycle, store_notification | Source-only captures for pending or explicitly held seams; no completed Rust parity claim |

The 73 Rust initializer candidates include 14 dummy credential groups that the
hosts do not read or return and eight collection/default/option groups, some
with different Rust types. Metadata pairs and expected alias equalities are
intentional. Independently varied native numeric matrices close their numeric
paths only. Shared decoder ownership does not discharge a worker's copied output:
retained domain payload cases above remain open, rather than being excused as
background fixtures. The Store owner independently confirmed that its consumer
measurement executes this crate's risk test target and does not import or spawn
this crate's risk oracle from a Store/types gate.

TWAP state creation and projection now each run 24 actual-source Market identity
cases over 19 AST leaf paths, including independent boolean probes and retained
info. Native Market fields are read through an explicit test-only projection,
shared with the task identity gate; no Market serializer supplies the observed
values. Separate compiling id/base swaps at both production copy sites fail
`TWAP-MARKET native identity` and `TWAP-PROJECTION native market identity`.
The source supplementary inputs come from the labeled task identity supplement,
inside this crate; the original task numeric artifacts remain unchanged.
Other copied-domain and nonnumeric projection dispositions remain open.

Review found both new TWAP identity captures initially recorded aliased inputs
after production execution. In-memory source id/base swaps then rewrote input
and expected together and both oracles exited zero. Inputs now clone imported
samples and snapshot before the call; expected outputs are cloned afterward.
Permanent source controls fail `SOURCE-TWAP-MARKET copy identity` and
`SOURCE-TWAP-PROJECTION market copy identity`, rather than letting a mutation
rewrite the reference input. Those earlier passing captures are not evidence.

Prepared and sanitized snapshots also receive 24 source-derived identity records.
Native assertions independently read Balance used/free/total/upnl, every required
Ticker scalar, Order scalar identity, Position scalar identity in both ordinary
and displayed arrays, and Market fields. Numeric values and string identities
are distinct within each record. Inputs are cloned before either source call.
Optional shared model fields not present in these fixtures are not claimed by
this supplement; the pre-existing info-presence witnesses remain separate.

The displayed-position supplement directly reads retained Position, Ticker and
Market fields from a record with distinct numeric and string values. Enrichment
adds both mixed reduceOnly/isStopped prototype cases and compares native retained
Order fields and enrichment extras against the source output. Deliberate all-zero
prototype falsiness cases remain intact. The earlier whole-Order codec comparison
is retained only as shape evidence.

## Subprocess inventory

Four numeric depth tests previously launched Node per case: six primitive task,
six fixture, four envelope and six complete-task cases (22 launches). They now
collect the same raw inputs and invoke Node once per depth corpus (four launches).
The large numeric, lifecycle and projection oracles already batch cases into one
JSON result. Dependency and source-mutation controls execute distinct oracle or
mutation configurations, rather than launching a process per data case.

## Capture provenance headers

All 30 mutable live source captures carry validation Node major/version and
architecture in their source header. The two preserved task generators retain
their original bytes; this audit and the labeled identity supplement carry their
Node 26.7.0 / darwin-arm64 provenance instead. Runtime metadata on risk, margin
and task identity captures reports both the executing architecture and Node major.
These headers identify validation hosts, not frozen cross-host golden bytes.

## ActiveChaser closure census

The live source AST declares 11 fields. All 11 are read from native output by
`chaser_instance`, without ActiveChaser serialization. Across 90 scenarios and
360 projected records, the AST identifies seven same-type field pairs: three
string pairs, three number pairs and one boolean pair. Every pair has at least
one case where both fields are present and differ. No pair remains swap-blind.

There are still 135 equal-value pair occurrences: size/attempts in 15 records and
reduceOnly/isStopped in 120. Their contrasting cases pin identity; deleting all
contrasts now fails the source pair census. The earlier repair changed one
blind pair, size/maxAttempts, in place because this artifact has no executable
consumer outside the worker crate. No shared feeder was rewritten. Enum fields
side and origin have different declared types and independent native matches.

The source shape must remain an explicit property-only type literal; new fields
or unsupported shape changes fail the census rather than becoming invisible.
This audit closes ActiveChaser only, not the other families still listed above.

The source control that restores `maxAttempts=size` fails
`CHASER-IDENTITY distinguish size/maxAttempts`. The production size/maxAttempts
swap compiles and fails `CHASER-PROJECTION native first case`; restoring it passes
the same scoped target. These are separate controls for the corpus and consumer.

## Price versus amount precision

The implemented worker surface has one precision-selecting formatter:
`chaser_manager::find_best_price`. Tracing its source and Rust arithmetic shows
the price step feeds the distance, spread clamp and final `adjust`; the only
other rounding site is TWAP's order-count floor, with no market precision input.
Pending placement functions are not covered by this implemented-surface audit.

The existing 479-case price corpus uses amount step 1 and varies the price step.
A production price-to-amount swap compiled and failed the original
`CHASER-PRICE 0` assertion, so zero consumers required a fixture-value repair.
That first failure involved a zero step. A new named native check selects four
existing finite cases: buy/sell, price step 0.01, amount step 1, distance 1/2.5,
and bid/ask 100/102. It reads the bits of the native returned price without
serializing Market. The same compiling swap now fails
`CHASER-PRECISION native price step differs from amount step`; restoration passes.
One consumer and one corpus examined; four existing cases given explicit
precision-identity assertions. The source artifact is byte-identical, with no
new capture, shared-feeder rewrite or retroactive independence claim.

## Enrichment retained-field supplement

The original `chaser_enrichment_oracle.mjs` remains byte-identical: 2547 bytes,
SHA256 `665d85e9b9d03f175dc4bad2f01b2db3143f21352e8269590cd5e4fab1900e6f`.
Its 24 cases
already pin ownership/prototype behavior; they did not independently observe
retained enums and optional fields after production enrichment. The separately
labeled `chaser_enrichment_identity_oracle.mjs` supplies 12 source cases with
24 owned/unowned orders. Its AST census requires all 21 Order properties and
nine extra properties, and fails on unsupported shapes or added fields.

All 43 compatible pairs (28 Order, 15 extra) have distinct present values.
Every source enum spelling is observed. Native output reads use exhaustive enum
matches, bit comparisons and scalar/optional field reads, never whole-Order
re-encoding or a typed expected Order decoded by the same implementation.
Source inputs are cloned before executing the real processor; expected outputs
are captured afterward. The supplement has no external consumer and makes no
retroactive independence claim about the original corpus.

Compiling production swaps fail `ENRICHMENT-OPTIONAL native tpTriggerType`,
`ENRICHMENT-OPTIONAL native extra.remainingNative` and
`ENRICHMENT-OPTIONAL native reduceOnly`; restoration passes. These respectively
exercise retained optional enums, numeric dimensions and boolean identity.
A fixture mutation equating TP and SL trigger values fails the source census at
`ENRICHMENT-IDENTITY distinguish tpTriggerType/slTriggerType`.
Snapshot and displayed-position optional fields remain separate open audit work.

The first supplement review rejected its unchecked zip cardinality. Removing
all unowned expected outputs left every pair/enum census green and the native
test exited zero, comparing only 12 of the claimed 24 records. Both boundaries
now count: the source requires 24 outputs and two per case; native code requires
expected and actual lengths to match before zipping. Dropping unowned outputs
fails `ENRICHMENT-IDENTITY total output count`. Bypassing that source check in
the same mutation independently fails `ENRICHMENT-OPTIONAL expected/native order count`.
The prior green was not evidence for unowned record identity.

## Zip cardinality follow-through

The worker Rust test census finds seven zip sites. Inspection
of their enclosing comparisons finds three recursive comparators with length
checks, the original enrichment comparison followed by whole-vector equality,
the PnL clock comparison followed by whole-array equality on every clock-identity
scenario, and the new enrichment and older leverage comparisons. Only the last
two lacked an independent cardinality check; both are now guarded before zip.
Four separate production zip sites are not fixture comparisons: ticker baseline
copy/packing and the two position-equality comparisons check lengths first.

The older leverage target passed all 2058 cases with a compiling production
mutation replacing the position patch with an empty position-list update.
The update-ID delta still matched while zip compared zero records. The repaired
test pins retained length and each position ID before numeric comparisons.
The same mutation now fails `LEVERAGE-CARDINALITY retained positions`, then the
restored operation passes. This is a test-coverage repair, not a production
behavior change or a claim that every retained Position field has been audited.
The original leverage source artifact is unchanged, SHA256
`2f8eda3c964ce75f18f128a3e98c78398ff69a2b7c10b88de04c140257edc3e6`.

## Nested corpus cardinality

The active-symbol oracle has 108 cases with four steps each. The old consumer
asserted only the outer count: removing both steps and expected results from
one case still passed. Native execution now has a fixed four-step bound, exact
source and expected per-case counts, and an explicit 432-comparison assertion.
Empty and one-step subsets fail `ACTIVE-CARDINALITY steps per case`; empty and
one-case outer subsets fail `ACTIVE-CARDINALITY source total`. Restoring the
byte-identical source oracle passes. This is a measured inner-loop false green,
not evidence that other worker corpora are bounded. Snapshot preparation, ticker
traces, displayed-position steps and several identity supplements remain under
the separate whole-corpus/per-case census.

Read-only cardinality census at `ed1d33b33c` examined 38 Node-invoking Rust
files and traced suspect loops into their generators. Recorded bounds and remaining repairs:

| Consumer | Independent expectation |
| --- | --- |
| store_preparation | repaired below: 72 scenarios × 17 steps = 1224 |
| store_payload | repaired below: 70 scenarios / 354 actions |
| displayed_positions | 199 scenarios / 222 steps |
| task_identity | repaired counts below; collection proof is scoped to its two singleton fields |
| task_types | ingress token inventory independent of the 20-field census |
| postprocessors margin supplement | 3 rows |
| message_stats | 9 optional / 8 primary shapes |
| worker_buffer | 7 numeric / 4 Unicode probes |
| worker_logging | 598 records / 2 nonfinite / 6 overflow probes |
| worker_error | supplemental text and clock-bit inventories |

Chaser-instance/subscription whole-array equality catches unilateral deletion but
does not independently bound source actions regenerated with their expectations.
Dependency discovery's minimum of nine also does not pin the complete inventory.
Numeric matrices already count 810/450/3420 comparisons and chaser-manager counts
7488 transitions; those totals are not per-field/per-token completeness claims.
This is a targeted census, not a worker-wide clean verdict.

The capture-bit target formerly passed with both an empty source token list and
a one-token subset. It now pins 11 source rows, two cells per row, the ordered
11-token native inventory, and 11 executed comparisons. Empty/subset source
controls fail `CAPTURE-CARDINALITY source total`; bypassing that guard still
fails `CAPTURE-CARDINALITY missing independent token`. Compiling empty/subset
native-loop controls fail `CAPTURE-CARDINALITY native comparisons`; an extra
native iteration fails the independent-token bound. Restored code passes.
These are consumer-test mutations; the scalar production capture operation has
no variable-length output. Existing NaN-only normalization is unchanged.

Snapshot preparation and ticker bookkeeping both passed their old targets when
one scenario's steps/actions were emptied or shortened to one. Their repaired
consumers use independent per-scenario bounds: 17 for every preparation trace;
5 for the first 66 ticker traces, then 8/8/2/6 for the four special traces.
Execution counters require 1224 and 354 comparisons respectively. Source arrays
are unchanged; the repair lives in their Rust consumers. Native complete-output
equality remains in place, with explicit collection lengths before comparison.

Snapshot empty/subset controls fail `SNAPSHOT-CARDINALITY steps per scenario`;
bypassing that guard fails `SNAPSHOT-CARDINALITY missing fixed step`. Empty and
partial outer corpora fail `SNAPSHOT-CARDINALITY scenario total`. Compiling
production empty-position, partial-order and extra-order mutations fail
`SNAPSHOT-CARDINALITY expected/native positions` or `expected/native orders`.

Ticker empty/subset controls fail `TICKER-CARDINALITY actions per scenario`;
bypassing both action and expected source guards fails
`TICKER-CARDINALITY missing fixed action`. Compiling production empty-delta
and extra-delta mutations fail `TICKER-CARDINALITY expected/native deltas`;
truncated delta values fail `TICKER-CARDINALITY complete output`. Both restored
targets pass. These controls do not mutate serde collection consumption and are
not decoder coverage for the consuming-deserializer defect class. That audit
remains separate and open; a serde trailing-element refusal is not its proof.

## Task identity corpus bounds and collection controls

The six source record inventories are pinned independently in the Rust consumer:
case/leaf counts 5/3, 19/12, 10/8, 5/4, 47/33 and 44/31 for ChaserTerminate,
ChaserOrderOptions, ChaserCompleteEvent, ChaserOrderState, TwapOptions and TwapState.
Each uses four decode routes, fixed case/leaf bounds, unique leaf paths, required
source pointers and an executed-comparison total (13032 across the six records).
All 24 empty/one-item case/leaf truncations fail their record-specific
`TASK-CARDINALITY cases` or `TASK-CARDINALITY leaves` assertions. Original source
artifacts remain byte-identical; no independent-source history is claimed.

The old gate already rejected a consuming-empty shared Array decoder at
`TASK-IDENTITY native field ChaserOrderState/currentOrdersIds/0`. Its actual gap
was extra decoded elements: a compiling duplication of each array's first element
passed the old target. Explicit source/native length-one assertions now reject
that mutation at `TASK-CARDINALITY native collection`. Removing both length
assertions while retaining the extra-element mutant returns the target to green.
Removing them with the consuming-empty mutant still fails the older field check;
those guards are load-bearing for extras, not the sole protection against empty.

The consuming mutation is `self.0.by_ref().for_each(drop); Ok(None)` inside
`numeric_ingress::Array::next_element_seed`, with the unused seed renamed. It
consumes every input and therefore cannot rely on a serde trailing-element error.
The extra-element control duplicates the first value in the shared Node::Array
conversion before decoding. Both production edits are temporary and restored;
only consumer assertions are retained. These singleton fixtures do not establish
nonempty-subset coverage for longer collections or any other worker decoder.

Each collection was exercised independently: moving TwapState first makes both
consuming-empty and extra-element controls fail its own
`TASK-CARDINALITY native collection TwapState/ordersSizes` assertion. Four
empty/subset controls with source case/leaf guards removed still fail the fixed
case or leaf access assertions. Restored tests and separate host/wasm strict
Clippy pass. The worker all-target check against integration a09ea6489b passes
on re-enumeration iteration 1 with zero new diagnostics; this is scoped worker
evidence, not a workspace build claim.

## Displayed-position execution bounds

The old displayed-position target passed both an oracle with every inner step
removed and one with the nine-step scenario truncated to one. Outer inventory
checks retained the six clock cases, one identity case and eleven callable cases,
but their presence did not establish that any step ran. The source bytes remain
unchanged; the Rust consumer now pins 199 scenarios, six independent multi-step
bounds (9/5/3/6/3/3), one step elsewhere and 222 executed step comparisons.
Clock rows have a native length guard before their field-wise zip.

Empty/subset outer controls fail `DISPLAYED-CARDINALITY source total`. Inner
empty/subset controls fail `DISPLAYED-CARDINALITY steps per scenario`; removing
that guard while retaining either mutation fails `missing fixed step`. Compiling
production clear, truncate-to-one and duplicate-first-output controls fail
`DISPLAYED-CARDINALITY native array`, at scenario 0 or 99. The restored target
passes. These are output and execution controls, not consuming-deserializer
coverage. Original capture and production files are byte-identical after restore.

## Scalar-ingress token coverage

The original task-types target passed when ingressNumbers was empty and when it
retained only one token. Its 20-field census inserted field names before the
loop, so field coverage did not prove that numeric comparisons ran. The source
artifact is preserved byte-identically. The consumer now executes seven fixed
tokens across 20 native fields and three decode routes, requiring 420 native
bit assertions, with one counter increment after each assertion.

Empty/subset source controls fail `INGRESS-CARDINALITY token total`; bypassing
that guard fails `INGRESS-CARDINALITY missing fixed token`. Compiling native
consumer-loop empty/subset/extra controls fail
`INGRESS-CARDINALITY native bit comparisons`. Keeping the empty loop while
removing that assertion returns the target to green, proving the counter guard
is load-bearing; restored code passes. These are consumer-loop controls, not
production decoder or consuming-deserializer mutations. This closes only the
scalar token axis; record-field, optional-field and union inventory loops remain
separate census items, and no whole-target cardinality claim is made.

## Task-record inventory bounds

The previous task-types consumer passed empty and shortened field inventories,
and empty and shortened ChaserTerminateKind literal lists. Four baseline controls
exited 0. The preserved task_types_oracle.mjs remains byte-identical; independent
consumer inventories now require all six records' exact field and optional names.
Fixed native probes execute 53 field removals and 14 optional-null assignments,
with per-record counters 3/0, 10/5, 8/1, 4/0, 15/4 and 13/4. Each probe requires
its sample key to exist, so omission cannot turn the probe into a no-op.

Six empty/subset source controls fail named RECORD-CARDINALITY source fields,
source optional or source termination literals assertions. Exact source-inventory
equality is the coverage anchor. Editing the fixed literal list itself also fails
there; that is distinct from the six compiling iterator mutations actually run:
`fields.iter().take(0)`, `.take(1)`, `.chain(fields.iter().take(1))`, and the same
three for optional. Those preserve the lists but skip or duplicate execution,
and fail RECORD-CARDINALITY executed field and optional checks. The counters
are loop-execution guards against such skips or future early continues; they are
not independent evidence of field provenance. Deleting one record call site is a
structural control: comparison of the five-element result array with the fixed
six-element expected array fails compilation with E0277.

Termination checks read three native variants, then independently check all three
wire spellings, then execute 15 rejection probes (21 checks). The price/percent
serde-name swap fails RECORD-IDENTITY native termination price. A Serialize
implementation emitting price for every variant fails RECORD-WIRE native
termination percent. Both directions remain required; native-first ordering
prevents a symmetric rename from cancelling through serialization. The restored
`cargo test -p safe-cex-webworker --test task_types` target passes. This is not a
full-crate pass: handlers has a pre-existing 19-versus-20 source inventory failure
at both this PR's base and original head.

The separate lexical -0 input assertion now has its own 20-check count, alongside
420 decoded-field bit assertions. Skipping that branch fails
INGRESS-CARDINALITY lexical negative-zero inputs rather than leaving its work
outside the counted inventory.

These controls prove consumer inventory execution and one native enum identity
axis, not consuming-deserializer collection coverage or a whole-target audit.
The five shared union lists remain outside this positive-literal census. Existing
sample roundtrips remain shape evidence; task_identity supplies direct record
field comparisons separately. No source artifact or fixture value was rewritten.

## Margin-symbol fetch effects

The new crate-local live-source corpus contains 32 independently pinned input
records, including all twelve stock prototype names. Four separate boundary
probes retain numeric symbols, a lone-surrogate Store key, a null leverage entry
and a custom prototype. No existing capture is rewritten, and this supplement
feeds no gate outside the worker crate. The emitted header records architecture
and Node major. All numeric inputs in this finite corpus are explicit constants;
this corpus makes no NaN-payload claim.

Native Store fields and error metadata are checked before reply/state wire
comparisons. Fixed per-case fetch, notification and collection counts prevent
empty or shortened source output from reducing the expected work. Same-length
reversal and replacement controls fail `MARGIN source input content`. An own-only
production lookup fails `MARGIN independent fetched count 19`; omitted catch
cleanup fails the same assertion at case 8. Swallowing the first null-result
clear failure fails `MARGIN native notifications 11` (two versus three).
Cancellation tests distinguish unpolled, pending and published-result states,
and an old future dropped after a same-symbol restart must preserve the new
marker. These are scoped helper observations, not a claim that UiStateHandler's
outer dispatch or source detached scheduling is implemented.

The added numeric-key scenario first failed `MARGIN native fetched key order 31`:
Rust retained OTHER/42 while the source enumerated 42/OTHER. Publishing and
clearing now use the shared js_own_keys order without a JSON roundtrip. This
property-order comparison reads native keys; map-value equality cannot see it.

At reapply commit 18b5fe3dff, removing normalization only from
clear_fetched_leverage survived all four tests (exit 0): the numeric placeholder
case did not exercise cleanup. The separate cleanup-order supplement now
publishes OTHER/BTC, inserts a concurrent 42 key after both non-index keys, and
finishes a null-result fetch. Native assertions pin the unsorted host precondition,
two retained entries in 42/OTHER order, their numeric values, one fetch and three
notifications before source projections. Its separate oracle executes the actual
UiStateHandler and DefaultStore; the original 32-case oracle is byte-identical.

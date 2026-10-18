# Binance field-identity and NaN capture audit

Current scope: FINAL_FIX_AUDIT.md retires the four unused Futures/PM wire
codecs and their standalone record/depth/field-identity fixtures. References to
those fixtures below are historical receipts at the stated revisions. All live
native binding and surviving union/inverse guards remain in force.

Baseline: `48a5682a73`; capture host darwin-arm64, Node26.7.0. This is a new
audit and supplement, not a retroactive independence claim for earlier fixtures.

## Inventory and method

41 tracked JavaScript scripts:38 standalone JSON-producing source corpora, two
stdin-driven corpora (`signing.mjs`, `public-api.mjs`), one control runner
(`signing-controls.mjs`). The uncommitted PM setter corpus is outside this claim.
38 Rust test files and zero checked-in JSON goldens were independently reviewed.
A literal Rust json! screen parsed63 object fixtures and found214 equal scalar
sibling-pair occurrences; dynamic expressions were excluded from that screen and
reviewed separately, not counted as absent.

Reproduce the screen with Python3.12 `scripts/field-identity-census.py`; it pins
the baseline script inventory and writes the full per-pair enumeration under
`target/binance-field-census.json`. It fails loudly on a failed oracle.

The executable JSON census recursively compares same-typed sibling fields,
projects numeric observation wrappers, and opens raw/rows JSON strings. It
examined489725 object observations and2118 path/field/type pair groups in the
38 original standalone corpora;717 groups have at least one equal observation,
438 have no differing observation within that path. These counts include
source-invariant outputs and fixture metadata, and are NOT defect counts.
Raw JSON parsing in this screening is not a signed-zero oracle; the separate
f64-bit assertions remain authoritative. Array positions and object-valued
fields are outside the scalar-pair screen and were inspected in source.

| Corpus | Objects | Pair groups | Equal groups | Always equal groups |
| --- | ---: | ---: | ---: | ---: |
| apply-depth.mjs | 86840 | 15 | 4 | 0 |
| buffered-depth.mjs | 5130 | 1 | 0 | 0 |
| closed-pnl.mjs | 76824 | 78 | 28 | 2 |
| closed-position-reader.mjs | 11737 | 88 | 10 | 2 |
| dispatch-findings.mjs | 480 | 20 | 9 | 3 |
| dispose-notification.mjs | 64 | 7 | 7 | 2 |
| futures-orders.mjs | 13759 | 176 | 11 | 0 |
| margin-mode.mjs | 60 | 4 | 1 | 0 |
| numeric-depth.mjs | 36 | 210 | 194 | 194 |
| numeric-ingress.mjs | 324 | 118 | 99 | 97 |
| order-history.mjs | 102545 | 93 | 45 | 0 |
| order-logic.mjs | 492 | 9 | 1 | 1 |
| orderbook.mjs | 34500 | 0 | 0 | 0 |
| params.mjs | 12478 | 16 | 8 | 0 |
| parsers.mjs | 7083 | 1 | 0 | 0 |
| pm-orders.mjs | 8251 | 151 | 10 | 1 |
| position-mode-store.mjs | 8 | 10 | 10 | 0 |
| position-mode-worker.mjs | 1359 | 3 | 0 | 0 |
| position-mode.mjs | 346 | 8 | 3 | 0 |
| private-open.mjs | 3080 | 30 | 25 | 0 |
| private-ping.mjs | 480 | 9 | 4 | 0 |
| public-connect.mjs | 48 | 10 | 6 | 0 |
| public-dispose.mjs | 64 | 7 | 4 | 0 |
| public-open.mjs | 5048 | 12 | 10 | 0 |
| public-start-store.mjs | 24 | 13 | 13 | 4 |
| public-start.mjs | 714 | 27 | 26 | 9 |
| public-stats.mjs | 3335 | 27 | 3 | 0 |
| public-subscriptions.mjs | 7430 | 3 | 0 | 0 |
| rejection.mjs | 26491 | 5 | 3 | 0 |
| response-message.mjs | 41324 | 14 | 10 | 0 |
| response-shape.mjs | 4710 | 113 | 106 | 106 |
| send-store.mjs | 192 | 52 | 37 | 13 |
| spot-open.mjs | 2208 | 11 | 4 | 0 |
| spot-orders.mjs | 4252 | 68 | 5 | 0 |
| spot-protocol.mjs | 9452 | 6 | 1 | 0 |
| state.mjs | 7174 | 12 | 6 | 0 |
| trailing-orders.mjs | 11346 | 77 | 10 | 0 |
| types.mjs | 37 | 614 | 4 | 4 |

## Findings and disposition

**One demonstrated false-green family repaired by a separate supplement, across
four public wire records (51 fields,169 same-Rust-type pairs).** Five numeric
pairs already had independent numeric cases; sibling strings did not. A Futures
Deserialize-only position_amt/entry_price swap compiled and the old types plus
numeric-ingress suite passed9/0/0. Original bytes were restored. Five subsequent compiled swaps fail the new
`public wire field identity` assertions (Futures/PM positionAmt-entryPrice,
Futures optional initialMargin-positionInitialMargin, income info-tradeId);
options startTime-endTime fails earlier at the shared `numeric ingress changed
Number bits at $.startTime` guard. Restored depth/numeric/types baseline11/0/0.

`field-identity.mjs` uses the actual source schemas, gives all sibling string and
number fields distinct values, and supplies every optional field. The new test
reads each public decoded field directly after seven raw/owned/borrowed/shared
routes. It does not serialize the enclosing record: a symmetric Deserialize/
Serialize rename cannot cancel the mismatch. One enum and one boolean at most
occur per record, so no same-record enum/enum or bool/bool pair is omitted.

Original numeric-ingress.mjs and numeric-depth.mjs remain BYTE-IDENTICAL to the
baseline. The former feeds the latter inside this crate. Repository reference
search found no Binance fixture/corpus data consumer outside this crate; root
CI commands execute its tests but do not feed their data into another oracle.
No shared corpus or golden was regenerated. Supplement is new evidence only.

Remaining always-equal groups were classified by source operation:

- Closed-PnL and reader: time/closedTime are the same source time, entryPrice/
  leverage are literal zero. Swapping identical source outputs is not a defect.
- Dispatch: time/timestamp, inherited hit/stat identity, arity/count are explicit
  source relationships. Existing own-shadow and arity cases distinguish inputs.
- Disposal: removed listeners yield zero close callbacks AND notifications;
  the retain-listeners control already distinguishes violating that behavior.
- order-logic input/expected false is the intentionally disabled source helper.
- PM payload reduceOnly/closePosition are both literal true when both present;
  cases with only one field present already distinguish their identities.
- Startup resets balance fields to literal zero and three loaded flags to true;
  initial loaded masks vary independently in public-start. Store-only witness
  fixes market/ticker flags false because it measures notification ordering.
- response-shape key-order fixtures use all1 values because this operation
  observes own-key names/order, not those values. Separate msg/code inputs vary.
- send-store health zeros/booleans are derived empty socket-registry state in
  a source-only reachability witness; this is not a health-field mapper claim.
- types tables contain source aliases (same endpoint or enum result); keeping
  them equal is the source contract, not an independently writable input pair.
- Numeric/depth generated string defaults are swap-blind: covered by the NEW
  whole-public-field supplement above, not relabeled as previously independent.

Other manually inspected fixture contexts: formatter Market.id==symbol is unused
as id by current formatter bodies; options.symbol and precision are the actual
reads. Zero ticker/position context fields are unconsumed; exercised last,
contracts and precision dimensions vary independently. Dirty orderbook triples
are overwritten and snapshot output uses distinct price/amount/total inputs.

Stdin corpora: signing485 main vectors plus8 alias vectors and targeted tests;
key/secret and timestamp/window differ, alias equalities are source identity
witnesses. Public factory16vectors: perMinute==total==1 follows a single request,
and tunnel flags deliberately correspond to configuration. Control runner
consumes these signing cases rather than supplying a third golden.

## NaN payload inventory

Four JS bit-capture corpora and five Rust bit-observation test files examined:

| Corpus/test | NaN reachable in captured bits? | Disposition |
| --- | --- | --- |
| numeric-ingress / numeric_ingress | No:18 valid JSON numeric lexemes give finite, signed zero or infinities | Original bytes preserved; bit-exact |
| numeric-depth / numeric_depth | No: selected captured value is exactly1 | Original bytes preserved |
| closed-position-reader / closed_position_reader | No: NaN times skipped; monetary parseFloat non-finites become0; finite product may overflow to Infinity; NaN bounds issue no GET | Existing bits retained |
| public-api / public_api | No in16 measured response fixtures: JSON-derived scalars, finite123 assertion | Existing capture retained; not a universal arbitrary-callback claim |
| orderbook native snapshot-ID test | Yes: native NaN included | Canonical NaN-only capture; every non-NaN bit retained |

Other numeric corpora use explicit String(number) with -0 detection (NaN becomes
the single string NaN), not CPU NaN payload snapshots. This audit does not claim
x86 execution. Native orderbook capture now canonicalizes sign and payload only
for is_nan, includes both NaN signs and a noncanonical payload, and retains
±0, ±Infinity and finite1.5 bit-exact. Compiled controls fail `signed-zero capture retains minus zero` when zero signs
are flattened and `finite capture retains payload` when non-NaN finite bits
are suppressed. Exact restoration passes. Reviewer MERGE for this audit.

## Roundtrip symmetry audit

The38 baseline Rust test files were inspected for decoded values observed only
through Serialize or re-encoded bytes, including generic union helpers. Four
additional test sites needed native assertions: types (19 income variants),
spot_protocol (two API and three orderbook variants), params (all six public
fields), and apply_depth (the operation's returned orderbook variant). The new
four-record supplement already inspects each public field, but its enum-valued
income_type projection alone cannot establish native variant identity; the
explicit19-variant match closes that separate hole.

Serialization checks remain alongside native assertions. Positional rejection
uses serialization only to construct a forbidden sequence; it is not a positive
identity claim. Tables and Map/Value outputs have no separate native enum or
record serialization permutation. No original oracle corpus bytes were changed.

## Capture engine versus CI engine

The captures counted here ran on Darwin arm64 / Node26.7.0. The Binance CI step
runs in the TypeScript job configured for Node22.x (the reported runner version
is22.23.1); shared primitives separately pin24.20.0. These are different engines,
not an assertion that this local runtime matches CI.

Zero frozen JavaScript runtime-output corpora are compared cross-engine: all38
baseline standalone corpora and both stdin corpora execute Node anew inside the
Rust test invocation. The added field-identity supplement also executes anew.
No tests read checked-in JSON goldens or captured output files; inspection of
include_str/include_bytes/file reads and command invocations confirms this.
The ignored local census JSON and isolated-mirror byte comparisons are local
measurements, not CI inputs. Native orderbook bit expectations are authored Rust
assertions, not frozen Node output; their NaN-only normalization is independent
of Node version. Live differential tests can still reveal runtime-version skew
against Rust; this audit makes no universal cross-engine equivalence claim.

The three generated Rust type/table modules ARE byte-checked on CI by types.mjs,
which regenerates declarations from the source schemas and constants rather than
reading a saved runtime corpus. The check passes on installed Node22.23.2 and
24.20.0 as well as the prior26.7.0 run. Exact22.23.1 was not run locally. Original
numeric feeder source files and all generated declarations remain unchanged.

Reviewer MERGE for the composed sweep. Five compiling roundtrip controls fail
`decoded income union native identity TRANSFER`,
`decoded protocol union native identity papi`,
`decoded protocol union native identity applied`,
`native params startTime case 6`, and `native depth result case 28`.
The first three pair decoding corruption with matching serialization corruption;
the params control pairs field assignment and serde-name swaps. Production bytes
restored exactly; scoped types/spot_protocol/params/apply_depth baseline9/0/0.

## Bracket precision follow-up at 325f4450d4

The earlier global independence audit did not establish branch-local independence.
Spot, Futures and PM varied amount precision only in cases without attached orders;
every bracket case had price and amount precision both0.1. A reviewer replaced
only bracket rounding's price precision with amount precision in each production
formatter. All three mutations compiled and each original live target passed1/0/0.
Production files were restored byte-identically before repair.

Each live corpus now appends12 labeled fieldIdentity cases: buy/sell, two distinct
precision pairs, and stop-loss-only/take-profit-only/both with fractional prices.
Direct native payload-field assertions precede whole-output serialization and
assert nonzero bracket coverage. The original trial prefix is unchanged; this
supplement makes no retroactive independence claim for previous green runs.
A repository reference search found only each script's own Rust test consumer;
none of these three corpora feeds another crate's gate. They execute freshly on
each host and store no frozen runtime output. The three repaired targets pass3/0/0;
reviewer verdict is MERGE. Six compiling controls failed the named bracket price
precision identity assertion: Spot1616/1617, Futures2720/2721, PM2288/2289,
respectively SL-first and TP-only. SL produced90 instead of90.4; TP111 instead
of110.7. Both fields are independently detected. Production bytes were restored
exactly and the final three targets passed3/0/0. This is three audited pair-sites, not a
claim that every branch of the102 ported rows has been checked.

The adjacent shared trailing formatter's current corpus distinguishes two
candidate pair-sites: compiled price-to-amount precision substitution fails successful
formatting case1 (callbackRate0 versus-0.1), and position-contracts-to-requested
amount substitution fails case0 (quantity999 versus3). Reviewer restored exact
production bytes; trailing target1/0/0. No repair, separate PM-only mutation,
or whole-venue claim follows from these two controls. This establishes coverage
now only: fixture ancestry against every earlier identity sweep was not audited,
so it does not establish that the pre-sweep corpus was never blind.

### Per-field scope and original captures

The bracket supplement covers attached stop-loss and take-profit outputs on limit
orders, both sides; it does not claim all order types or main-order price/quantity
coverage. Spot and PM use stopPrice for each branch; Futures uses triggerPrice.
Earlier unconditional mutations changed both bracket branches and failed first
on stop-loss. TP-only mutations isolated take-profit. SL-only controls also compile and fail: Spot stopPrice1616, PM stopPrice2288,
Futures triggerPrice2720, each90 versus90.4 while TP is unchanged. Together with
the TP-only controls, these establish six separately mutated output branches.
Production bytes restored exactly; the three-target restore passed3/0/0.

Original live outputs recaptured from source at325f4450d4 on darwin arm64,
Node26.7.0, remain byte-preserved under ignored target/binance-precision-baseline.
These are new captures of the original corpus, not historical frozen artifacts.
The supplemented outputs retain an equal original prefix and append12 cases.

| Corpus | Original cases | Bytes | SHA256 |
| --- | ---: | ---: | --- |
| Spot | 1616 | 687819 | e041ad470043855ebca0e20be1e73f0cbfefb034355aef6f9dd834cf2d8b8717 |
| Futures | 2720 | 1837805 | 38ba43558f8737b7295f4a6dcf9eac9e859b9a89dbce5eddbc9d89f11a871b50 |
| PM | 2288 | 1651486 | c10823a00b8a531181ccb41758c7e7dab8c02970cae09cf7fea0f13ffef4eadc |

## Collection cardinality audit

The current source census covers40 Rust test files and44 JavaScript scripts.
Four explicit zip sites were found: signing and the three bracket supplements.
Signing checks oracle output length against independently constructed inputs;
the formatter supplements check native payload length against expected length
before pairing. Whole payload equality remains an additional assertion.

The bracket supplements now also derive per-case source bracket counts from
independent request-field presence and require16 source brackets and16 native
field comparisons per family. The original12-case supplement sizes are asserted
separately. Signing's existing cardinality assertion now has a diagnostic name.
Reviewer MERGE: six production-payload empty/partial controls and six source
bracket empty/partial controls compile and fail the independent named count
assertions. Two signing output controls fail signing oracle output cardinality
at0/485 and484/485. These establish current coverage, not historical immunity.

A fifth candidate uses no zip: public disposal loops over source attempts.
The outer16-case assertion does not constrain the inner two-attempt sequence.
Empty and partial oracle attempts both survived the original disposal target1/0/0.
The repair pins32 source attempts, two per case, and independently drives a fixed
0..2 native loop with32 completed comparisons. Four repaired controls compile and
fail: empty/partial source at DISPOSE-CARDINALITY source total attempts, and the
same truncations with source guards bypassed at DISPOSE-CARDINALITY expected/native
attempt coverage. Source script remains byte-identical (SHA256
185e321411cd08873603c9fa75b17a11ce5623f7cd5e899b6e43b4a459f662b7).
All mutations restored; final five targets15/0/0, reviewer MERGE. An initial PM
unused_mut baseline compile failure was repaired before controls and is not proof.

The broader read-only census found13 missing outer totals and4 lower-bound-only
totals. These remain a separate next slice; no whole-venue clean claim is made.

### Exact outer totals follow-up (local)

The13 missing outer totals and4 lower-bound-only totals now have explicit named
SOURCE-CARDINALITY assertions. Counts were measured by one live Node subprocess
per corpus on darwin arm64 Node26.7.0; no corpus source was edited for this repair.
All34 empty and one-row-short captured-source controls compile and fail their
named SOURCE-CARDINALITY assertions; restored17-target run20/0/0, reviewer MERGE.
These are source-harness mutations, not production-return mutations. Independent
completed-comparison counters were subsequently added after native assertions,
including params' special-value early-continue branch. All34 source-guard-bypass empty/subset controls compile and fail their own
NATIVE-CARDINALITY completed-comparison assertion; restored17-target run20/0/0,
reviewer MERGE. Original test bytes were restored exactly after each phase. Neither layer establishes nested-array or field-identity
coverage by itself.

| Target | Required source cases |
| --- | ---: |
| apply_depth | 3080 |
| buffered_depth | 2565 |
| spot_protocol | 9450 |
| response_message | 11828 |
| trailing_orders | 3794 |
| order_logic | 217 |
| response_shape | 1188 |
| orderbook | 3132 |
| state | 7174 |
| rejection | 5471 |
| params | 4161 |
| order_history | 4754 |
| parsers | 3534 |
| closed_pnl | 7239 |
| spot_orders | 1628 |
| futures_orders | 2732 |
| pm_orders | 2300 |


## Collection alias consumer

The Futures and PM GetPositionRiskResponse aliases are Vec records; the earlier
numeric corpus decoded individual rows and did not exercise alias cardinality.
The separate collection_cardinality target reuses the unchanged field-identity
source schema corpus, builds two distinct-symbol rows, and decodes each public
alias through seven ingress routes. Fixed variant, route and row inventories
require 28 native symbol comparisons; source alias identity is checked separately.
This is a new supplement, not evidence that the earlier row corpus covered arrays.

Injected shared Array::next_element_seed mutations consume remaining input before
returning None. Empty and one-row-subset controls compile and fail native length
at Futures shared raw route 4. Removing that length check while retaining each
mutant fails independent native index coverage. Neither relies on serde rejecting
unconsumed trailing elements. Baseline and restore each pass 1/0/0, and shared
production bytes are restored exactly. These are test-sensitivity observations,
not production decoder defects or independent mutation proof for every route.
The shared Array mutator belongs to num; no venue decoder shim is introduced.


## Callback and filtered-iteration follow-up

At dbdf3d15dd merged into afeebfcecd, the real manifest inventory remains 102
ported rows: 42 const, 35 field-fn, 3 method, 17 const-fn and 5 type. This is a
row census, not proof that every row has passed every mutation class.

Read-only inspection found 12 explicit Rust filter sites: three formatter
supplement selections require 12 cases each; three bracket selections check
per-case and total counts; send-store selections require 12/8/40/4; collection
aliases require one record per fixed variant; Spot protocol compares the complete
literal vector. This does not establish coverage for all conditional predicates.
The inspected mode/start/open/reader callbacks expose effects through outer counts
or trace equality; that inspection is not a compiled callback-bypass proof.

A targeted cancellation mutation found a separate vacuity gap. Omitting runtime
task enqueue only for inline=false compiled and passed the original
public_connect_disposal_cancels_unpolled_and_pending_dial test (1/0/0). Its zero
expected dial/drop counts and all() over tasks permitted no registered task.
This is a test-runtime mutation exposing an assertion gap, not a production
registration defect. The source oracle and production files remain unchanged.

The repaired cancellation test pins one pending registered task and retains its
Rc independently of the runtime collection. It asserts cancellation and future
release before pumping, then rechecks the retained task afterward. The original
skip-enqueue control now compiles and fails CANCELLATION-COVERAGE registered task
count inline=false (0 versus 1). A separate no-op cancel control compiles and
fails CANCELLATION-COVERAGE retained task cancelled inline=false. Repaired baseline
and exact restore each pass 1/0/0; no production mutation remains. The controls
prove these two test-runtime omissions, not whole-venue callback coverage.


## Main price and split quantity precision supplement

At 115f2f1c82, twelve isolated production axis swaps compiled against unchanged
formatter fixtures. Four survived: Spot main price, plus split remainder in
Spot/Futures/PM. Futures/PM main price and all three initial amount swaps failed
case 542 (zero step/max), so those five results prove edge discrimination only.
All three lot-size swaps failed case 636 (amount 10, amount step 1, maximum 3),
with native quantity 2.3 versus expected 2. Production restored exactly; grouped
original-target restore 3/0/0. These are current-corpus results, not never-blind claims.

The separately selected --precision-identity supplement uses eight limit-order
cases per formatter: both sides across four fixed tuples, fractional main price,
distinct price/amount steps and fractional split limits. Its Rust input admission
checks the full source input content by index; native assertions check price,
quantity, side, payload count and both precision steps before wire comparisons.
Default oracle bytes remain unchanged. Capture provenance (Darwin arm64, Node
26.7.0) is not an admission gate; the per-case content assertions are the gate.

| Original corpus | Cases | Bytes | SHA256 |
| --- | ---: | ---: | --- |
| Spot | 1628 | 696621 | bde7f3ae7b2b50675d1be236f8a8e9260f652d52de688d916b1bf46b1dcb6270 |
| Futures | 2732 | 1849435 | ebbd6fe12340a70fe17faefb9f0d3cddeb199568671899096f16fbd9c2766530 |
| PM | 2300 | 1662708 | eaebb13fa38a4a389b615a1508cf4545aac934cb2defc9ac16a950ada1c17d56 |

The supplement and corrected control runner received read-only sub-agent review.
At 8aebd755c0, twelve production controls compile and fail independent native
assertions: main price 12 versus 12.3, initial payload count 1 versus 2, split
remainder count 3 versus 2 and lot-size quantity 4 versus 3.6, independently for
Spot/Futures/PM. Six same-length/reversal source harness controls also fail the
fixed-index content assertion. Enumeration stops after these selected 18 controls;
no whole-102-row claim follows. Byte restoration and the three-target six-test
baseline/final restore pass. Exact SHA-bound exits, including the first runner's
ambiguous-selector stop, are in PRECISION_GATE_EXITS.json and GATE_EVIDENCE.md.
Default outputs were independently recaptured against 115f2f1c82 and remain
byte-identical with the sizes and hashes above (Darwin arm64, Node 26.7.0).

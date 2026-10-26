# Fixture identity and NaN dispositions

The JSON walk examined 170,293 records from ten frozen files and nineteen live
providers: 40,519 equal-value groups, 301 distinct field-pair candidates. Of those
pairs, 212 also occur with distinct scalar values; 89 do not in that JSON walk.
The expansion adds the later QueryValue profile capture and the existing runtime
account policy/Store stub. The original seven files were not rewritten. These
counts describe candidates, not 301 defects.

The native inventory parses Rust with syn, including recursive test directories,
inline source test modules and the four owned facade tests: 61 files, 180 functions,
29 native struct expressions, 121 literal JSON records, 24 expression-equality
groups and 71 dynamic JSON macros. It inventories native
struct expressions, literal JSON macros and dynamic macros separately. Expression
text equality is a candidate, not evaluated same-type equality. Dynamic macros
are logged with their function and expression; they are not silently counted as
literal fixtures. The inventory is a discovery tool, not an accepted-set gate.

The 71 dynamic macros have five dispositions below. The AST inventory refuses an
unclassified function and checks all five counts. This is a classification of the
measured macros, not an exhaustive assertion about future fixtures or resolved types.

## Dynamic macro buckets

| Bucket | Macros | What the macro does | Witness |
| --- | ---: | --- | --- |
| decoded-record | 13 | Constructs typed input, including variable-built task flags, negative numeric literals and the wire helper's per-field variations. Existing shape fixtures remain; distinct native-field supplements pin identity. | FIELD-IDENTITY Balance.used; symmetric used/free serde-name swap compiles and fails. |
| rejected-shape | 17 | Constructs malformed containers or altered closed-union spellings. These are negative input cases, not positive multi-field records. | DYNAMIC-REJECTION string union refuses non-string shapes; removing String-first ingress compiles and admits a formerly rejected object. |
| operation-result | 15 | Captures account, model, symbol and validation helper results. These are one-way comparisons against actual source operations, not matching decode/encode pairs. | DYNAMIC-OPERATION native base identity; swapping the Rust symbol tuple compiles and fails against the actual-source result. |
| numeric-observation | 9 | Emits scalar/tagged numeric observations or compares a numeric payload. Tags preserve NaN semantics; native finite and signed-zero bits remain exact. | CAPTURE-BITS finite payload remains exact and CAPTURE-BITS signed zero remains observable; independent normalization mutations compile and fail. |
| effect-observation | 17 | Captures classification, notification traces, Store transitions, book state, risk recovery and dictionary referent effects. Snapshot equality is not promoted to native-field identity evidence; the decoded-record and risk supplements cover those dimensions separately. | first subscriber failure must skip second and reach caller recovery; swallowing the callback failure compiles and changes the actual-source trace. |

The decoded-record bucket contains task-snapshot macros (2), PositionRecord's
record (1), wire variation macros (2), ticker fractional input (1), recursive
Balance/Execution inputs (2), update-union input (1), health/leverage inputs (2),
market construction (1) and terminal-order construction (1).

The rejected-shape bucket contains PositionRecord union negatives (6), private
codec shape negatives (6) and extra-model string-union negatives (5).
The operation-result bucket contains account outputs (5), model helper outputs
(5), symbol outputs (4) and validation-error retained output (1).
The numeric-observation bucket contains validation numeric tagging (1), derived
encoding (2), comparator controls (2), returns outputs (3) and book scalar encoding (1).
The effect-observation bucket contains instrument classification (1), trim/prototype
classification (3), notification traces (3), full Store state (1), book rows/sides
(2), sequenced state (1), terminal state (1), risk recovery inputs (3) and dictionary
referent input/mutation (2). This accounts for every macro in the measured inventory.

No original provider or frozen feeder bytes changed for these dispositions.
The added witnesses are later supplements; none claims retroactive independence.

## Field-value dispositions

- Order and Ticker zero sentinels, Balance totals, Position numeric fields, Market
  precision/limits, Candle OHLCV, Execution legs and request counters now have
  distinct-value native getter supplements. The original oracle bytes remain.
- StoreDataLoaded and ActiveTWAP flags use one-hot records. ActiveChaser and TWAP
  numeric/string dimensions, PositionRecord's required/optional numeric dimensions,
  and DisplayedMargin have direct native identity assertions.
- Store's socketHealth object gets four one-hot writes through the real Store;
  fetched leverage uses distinct buy/sell values. Packed update index, mask and
  values are read directly after decode.
- Risk's first fill can make orderPnl equal totalRealizedPnl legitimately. The
  separately labeled cumulative-fill supplement supplies a later profile with
  four distinct numeric dimensions and checks native getters against actual TS.
- Socket lifetime/streak integer-versus-fractional pair counts are separate Python
  categories; the existing recovered/unhealthy case uses totals 155 and 3 and
  the distinct source inputs 150/0 and 5/3. This is output computation, not a
  deserialize/re-serialize assertion.
- Account flag cross-products and symbol conversion combinations already contain
  differing inputs. The equality-only subset does not replace those cases. Open
  account Value helpers do not deserialize a typed same-name record on output.
- Reporter/provenance pairs (get-key input/entry/last labels, VM own/global/fresh
  observations and oracle configuration flags) are not shared model fields. Their
  actual source statements, branch controls and referent observations are checked
  separately; this audit does not claim a new model identity repair for them.

## Assertion-shape dispositions

| Gate | Assertion shape and change |
| --- | --- |
| types wire | Previously roundtrip-only for records. Fourteen model implementations now read each decoded native field before re-encoding; accepted per-field scalar variations distinguish sentinels. Nested field serialization alone is not claimed recursive identity; nested Market/task/history supplements read their actual native fields. |
| types PositionRecord | Original record test remains shape/spelling coverage; later supplement reads required/optional native fields. The facade numeric corpus already selects each native field directly in separate cases. |
| types p4_contracts | Constants read keyed native numeric values; fill roundtrip and writable projection remain projection checks, supplemented by domain identity tests. |
| types strict_wire / strict_codec_inventory | Refusal, literal inventory and private metadata checks; these are not positive record identity claims. |
| types p4_symbols | Actual conversion outputs; no matching typed record decode and encode pair. |
| types p4_accounts / p4_model | Actual operation output comparison and source errors. Account helpers return open Values; model helpers are operations, not model roundtrip proofs. |
| types validation_error / query_value | Actual-source return/query paths, native variant/value inspection; no whole-record symmetric roundtrip claim. |
| Store derived | Output-only comparator supplemented with direct native risk field reads and the later cumulative profile case. Crosshair fields are explicitly read into output tags. |
| Store contract | Packed/partial shape checks supplemented with direct native tuple and partial fields plus real-Store one-hot health/leverage reads. |
| Store p4_store | Actual state transitions, notifications and versions against source; domain field identity is separately supplemented, not inferred from snapshot re-encoding. |
| Store p4_orderbook / sequenced_book | Explicit native price/amount/total construction, transition/sequence/ready/recovering observations; these are algorithms, not matching decode/encode aliases. |
| Store instrument_class / notification | Classified native variants/labels and ordered subscriber effects/error identity; no serde roundtrip-only claim. |
| Store live_snapshots | Source drift only, preserving frozen bytes; it is not native identity proof. |
| owned facade native patches / risk recovery / dictionary writes | Native numeric getters, Results, refusal effects and source recovery. No claimed worker-oracle import; worker target is executed separately. |

The compiling symmetric Balance used/free name mutation fails both
FIELD-IDENTITY Balance.used (supplement) and WIRE-NATIVE Balance.used (wire helper).
Restoring it passes. A second compiling production swap exchanges risk orderPnl
and totalRealizedPnl: the original derived target remains 2/2 green while the
later cumulative supplement fails RISK-IDENTITY native orderPnl; restoration passes.
The latter uses per-field variation so the formerly identical
zero fixture now catches the corruption. No original shared feeder was regenerated;
all later supplements explicitly decline retroactive independence.

## NaN capture dispositions

The all-key hexadecimal scan found 116 candidates: 111 actual raw bit captures
and five decimal sequence strings (9007199254740991), with zero NaN payloads. It contains
176 stable NaN tags: the historical 170 plus six in the new QueryValue capture.
The current captures ran on darwin arm64, Node 24.20.0. Original frozen files lack
architecture metadata; no capture architecture is retroactively assigned to them.

The later eight-case bucket witness is separate from that nineteen-provider
inventory. Its four returns cases capture twelve bit fields, including one NaN
canonicalized at capture; each carries platform/architecture and Node major.
The source bytes are not an external feeder, so this new witness was edited in place.

- PositionRecord's 84 numeric JSON spellings and validation-error JSON lexemes
  cannot spell NaN. Their native bit observations cover finite values, overflow,
  underflow and signed zero without asserting a CPU NaN payload.
- Derived risk/returns, native patches, orderbook and sequenced-book NaNs are
  captured as `$number:NaN` or asserted by is_nan. Facade native patches branch on
  expected.is_nan before comparing bits. Dictionary VM global/referent results
  use named isNaN observations, not a bit snapshot.
- QueryValue tests inspect NaN by is_nan and negative zero by exact bits. Numeric
  query rendering is source qs output, not JSON projection.
- Constants, native identity supplements, the finite snapshot number and numeric
  utility JSON comparators only compare finite JSON number payloads in these
  fixtures. No valid JSON numeric lexeme is NaN.
- The later capture_bits supplement canonicalizes only NaN to 7ff8000000000000.
  Its negative-payload control is accepted; compiling zero/finite normalization
  controls fail CAPTURE-BITS signed zero remains observable and CAPTURE-BITS
  finite payload remains exact. Restoring both passes.

This preserves signed zero exactly. No blanket float equality, old feeder rewrite,
output projection skip or architecture-specific NaN assertion was introduced.

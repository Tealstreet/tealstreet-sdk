# Ticker REST-read slice

Rows: `BlofinExchange.fetchTickersRaw` and `BlofinExchange.fetchTickers`.
Tomllib measurement: **14 ported / 28 divergent / 83 pending**, 125 total.
The reads consume the shared configured RestAdapter; createAPI and complete
exchange registration remain pending. See the two manifest notes for the chosen
encoded-JSON-only/owned-output boundary and actual-source boundary observations.

The actual-source corpus executes three maintained method initializers, actual
roundUSD, actual DefaultStore and actual Axios boundary transforms in **one Node
subprocess**. It contains 26 fixed inputs × 3 emitter modes × 2 methods = **156
source cases**, each checked across three response variants = **468 native
comparisons**, plus 9 phase and 4 boundary comparisons. The capture records architecture,
Node major and full version (measured arm64 / 26 / v26.7.0).

The census is one comparison target with two returned collection paths (raw and
formatted), exact request and log vectors, and four ingress boundaries. Fixed
source totals/per-case counts, independent native lengths, whole native output
comparisons, input identity and per-index visit counts guard these paths. This is
not a venue-wide cardinality or precision claim. Neither new wrapper reads market
price/amount precision; no precision field swap is claimed for this slice.

## Mutation proof

`tests/reads_controls.py` measured **27 compiling runtime failures at 101 and 27
restored passes at 0**. Each run invokes one scoped `--test reads --jobs 1`;
no Cargo invocations run concurrently. Logs are in
`sdk-rs/target/blofin-reads-phase-controls/`. Compile refusals during runner development
were rejected as evidence; the final 27 pairs are runtime failures.

| Control | Named assertion |
| --- | --- |
| raw-endpoint | READ request payload |
| formatted-bypass | READ-CARDINALITY native outputs |
| error-precedence | READ native messages |
| emitter-discard | READ native fields |
| raw-empty | READ-CARDINALITY native outputs |
| raw-empty-source-bypassed | READ-CARDINALITY native outputs |
| formatted-empty | READ-CARDINALITY native outputs |
| formatted-empty-source-bypassed | READ-CARDINALITY native outputs |
| raw-subset | READ-CARDINALITY native outputs |
| raw-subset-source-bypassed | READ-CARDINALITY native outputs |
| formatted-subset | READ-CARDINALITY native outputs |
| formatted-subset-source-bypassed | READ-CARDINALITY native outputs |
| raw-extra | READ-CARDINALITY native outputs |
| raw-extra-source-bypassed | READ-CARDINALITY native outputs |
| formatted-extra | READ-CARDINALITY native outputs |
| formatted-extra-source-bypassed | READ-CARDINALITY native outputs |
| consuming-empty | READ-CARDINALITY native outputs |
| consuming-empty-no-length | READ native fields |
| source-empty | READ-SOURCE corpus total |
| source-subset | READ-SOURCE corpus total |
| source-extra | READ-SOURCE corpus total |
| source-nested-empty | READ-SOURCE per-case output |
| source-nested-subset | READ-SOURCE per-case output |
| source-nested-extra | READ-SOURCE per-case output |
| reparse-untransformed | READ response phase native value |
| reparse-transformed | READ response phase native value |
| short-native-walk | READ native comparison count |

The six source-guard-bypassed native controls restore production while retaining
the disabled source guards and pass. The consuming control drains every shared
numeric-ingress Array element before returning None; there is no trailing-element
Serde refusal. Removing the main-corpus source and native output length assertions while retaining
that mutant fails `READ native fields`; restoring production with those guards
still disabled passes. Shared production is restored byte-for-byte.

All original venue oracle files are unchanged. The new input pin is fixed rather
than regenerated during controls. The runner checks exact restoration of its
production, test, source-oracle and shared-decoder inputs. No grading policy is
implemented inside the venue.

Input pin SHA256: `36adb9a19c0af4b53b11b8c3723816d4dd88611942854983dc0095fcff3c7ac2`.

## Shared response-phase migration

Integration parent `a5954aa869e368638986261e9ffa8829797ac60b` changed RestResponse.body
from bytes to RestResponseBody. The two compile errors at the first merged head
were repaired by explicitly matching Encoded/Untransformed/Transformed. Encoded
JSON uses js_from_str; native values use js_from_value_ref without serialization
or reparsing. The configured adapter owns response transforms, not these readers.
Two separate native-phase reparse controls fail `READ response phase native value`.

The original 156 source cases were compared against the committed pre-cutover
oracle and are byte-identical: 63,815 bytes, SHA256
`b6b4683cd189dd9ad65aeea74b1db365ea143c0c11a87245899a4d9bedd6fde6`.
The oracle asserts that identity; three phase probes are a separate supplement.
All 27 controls were rerun after migration; the earlier 25-pair sheet is not
being attached to changed production bytes. Restorations avoid rewriting
unchanged shared files and triggering unnecessary recompilation.

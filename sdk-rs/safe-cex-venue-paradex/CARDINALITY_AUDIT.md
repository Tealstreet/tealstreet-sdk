# Cardinality audit

Baseline: local `61f48e170796aa32828218a4b2bd2e6238d2a7e2`.
PR 7255 remains pinned at `18878d8ca5556499c52b7dcb8e034e5dae2c62c6`.

## Measured classifications

Collection outputs were already covered: 26 compiling production controls
(13 collection sites, empty and subset at each) failed the ORIGINAL targets
with exit 101; all 26 restored runs passed. No production empty/subset false
green was observed. The only zip, supplementary `json_equivalent`, already
checks equal array lengths before walking pairs. Native observations compare
whole maps, and income observations compare whole arrays.

A separate comparison-coverage false green existed: changing the valid type
case loop to `iter().take(0)` or `iter().skip(1)` compiled and passed the original
target (exit 0 each). This did not empty production output: it skipped native
comparisons while the outer 15-record census still passed. Invalid/surrogate
checks continued. Do not report these as production collection controls.

## Repair

A count-only supplement captured from one execution of the actual TypeScript
(capture environment: arm64, Node v26.7.0 / major 26, V8 14.6.202.34-node.28,
darwin) pins each of 15 declaration names and its valid/invalid/surrogate counts:
354 / 1056 / 166 total. Exact unique declaration names prevent duplicate rows
from replacing a declaration. Counters increment after successful comparisons
and require six valid routes, three invalid routes, and one surrogate refusal
per source case. Predicates require 27 inputs / 54 comparisons; each lookup
requires 27 inputs / 27 comparisons.

Native Vec and tuple lengths are observed before serialization, separately from
leaf field observations, and compared with recursive actual-source array counts.
The supplemental serialization comparison retains its length-before-zip guard.
The utility supplement pins 230 income cases and their individual output counts
(432 total outputs). Native result length and resolver trace length are checked
independently against the live source before comparing fields. Native output
and completed utility comparison totals are asserted separately (120 scalar,
486 historical, 230 income comparisons). Error cases retain absent output
counts, distinct from successful empty arrays.

These supplements contain counts only. No existing fixture value was repaired.
All four existing oracle files remained byte-identical. Each scoped target
executes Node once for the entire corpus, and Cargo runs sequentially with
`--jobs 1`. This audit is not a workspace/wasm readiness claim.

## Original production controls

| Site | Mode | Named failure | Mutant / restored exit |
| --- | --- | --- | --- |
| income-empty | production | `income_native_fields "distinct-fields-supplement"` | 101 / 0 |
| income-subset | production | `income_native_fields "distinct-fields-supplement"` | 101 / 0 |
| OrderResponse-flags-empty | production | `native_field_observation ORDER_RESPONSE "all_fields" route 0` | 101 / 0 |
| OrderResponse-flags-subset | production | `native_field_observation ORDER_RESPONSE "all_fields" route 0` | 101 / 0 |
| BatchOrderResponse-errors-empty | production | `native_field_observation BATCH_ORDER_RESPONSE "all_fields" route 0` | 101 / 0 |
| BatchOrderResponse-errors-subset | production | `native_field_observation BATCH_ORDER_RESPONSE "all_fields" route 0` | 101 / 0 |
| BatchOrderResponse-orders-empty | production | `native_field_observation BATCH_ORDER_RESPONSE "all_fields" route 0` | 101 / 0 |
| BatchOrderResponse-orders-subset | production | `native_field_observation BATCH_ORDER_RESPONSE "all_fields" route 0` | 101 / 0 |
| BatchCancelOrderResponse-results-empty | production | `native_field_observation BATCH_CANCEL_ORDER_RESPONSE "all_fields" route 0` | 101 / 0 |
| BatchCancelOrderResponse-results-subset | production | `native_field_observation BATCH_CANCEL_ORDER_RESPONSE "all_fields" route 0` | 101 / 0 |
| OrderbookResponse-bids-empty | production | `native_field_observation ORDERBOOK_RESPONSE "all_fields" route 0` | 101 / 0 |
| OrderbookResponse-bids-subset | production | `native_field_observation ORDERBOOK_RESPONSE "all_fields" route 0` | 101 / 0 |
| OrderbookResponse-asks-empty | production | `native_field_observation ORDERBOOK_RESPONSE "all_fields" route 0` | 101 / 0 |
| OrderbookResponse-asks-subset | production | `native_field_observation ORDERBOOK_RESPONSE "all_fields" route 0` | 101 / 0 |
| MarginResponse-configs-empty | production | `native_field_observation MARGIN_RESPONSE "all_fields" route 0` | 101 / 0 |
| MarginResponse-configs-subset | production | `native_field_observation MARGIN_RESPONSE "all_fields" route 0` | 101 / 0 |
| BASE_URL-empty | production | `BASE_URL_entries` | 101 / 0 |
| BASE_URL-subset | production | `BASE_URL_entries` | 101 / 0 |
| BASE_WS_URL-empty | production | `BASE_WS_URL_entries` | 101 / 0 |
| BASE_WS_URL-subset | production | `BASE_WS_URL_entries` | 101 / 0 |
| ENDPOINTS-empty | production | `ENDPOINTS_entries` | 101 / 0 |
| ENDPOINTS-subset | production | `ENDPOINTS_entries` | 101 / 0 |
| ORDER_TYPE-empty | production | `ORDER_TYPE_entries` | 101 / 0 |
| ORDER_TYPE-subset | production | `ORDER_TYPE_entries` | 101 / 0 |
| ORDER_SIDE-empty | production | `ORDER_SIDE_entries` | 101 / 0 |
| ORDER_SIDE-subset | production | `ORDER_SIDE_entries` | 101 / 0 |

## Preserved oracle SHA-256

- `types_oracle.cjs`: `d5be38e829e1bda1826d3bca26503127eee4f616686cad4a471d8004ac770721`
- `utilities_oracle.cjs`: `c883ad6d62841ada85021900fc00c4a1777ed3e98e283ad4c717c246ca937345`
- `exchange_helpers_oracle.cjs`: `b88fed55a51090d8ba3d744917b528ef08f42f8e0bcd15953a718d9bacd8be66`
- `account_oracle.cjs`: `4c82101c6894c9c158d3549ad8860188e2ae3b23cc4f8f4d2dd00247680d411f`

## Source and comparison-loop controls

- `completed-empty`: exit 101, `type_completed_comparisons`; restored 0.
- `completed-subset`: exit 101, `type_completed_comparisons`; restored 0.
- `native-income-empty`: exit 101, `income_native_cardinality`; restored 0.
- `native-income-subset`: exit 101, `income_native_cardinality`; restored 0.
- `source-income-drop`: exit 101, `income_source_total`; restored 0.
- `source-income-bypass-counts`: exit 101, `income_native_cardinality`; restored 0.
- `source-types-empty`: exit 101, `type_source_total`; restored 0.
- `source-types-subset`: exit 101, `type_source_total`; restored 0.
- `source-duplicate-type`: exit 101, `type_unique_row_census`; restored 0.

## Consuming decoder control

The earlier 14 response-array controls mutated the returned native Vec AFTER
successful decoding (seven fields, empty/subset each). Input had already been
consumed, and failures were named native assertions, not trailing-element errors.
This census and that evidence are distinct from the shared SeqAccess control.

`python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_consuming_cardinality.py`
injects a mutation into shared `Array::next_element_seed`: exhaust the iterator,
then return `Ok(None)`. It restores shared source and test bytes in `finally`;
no shared production change is committed. The permanent aggregate target reads
one actual-TS corpus and checks seven fields across five decoded response owners.
Each all-fields source length is independently pinned to two, each native length
must be two, and exactly seven field comparisons must execute. Aggregation
reports every mismatching field rather than stopping at the first owner.

These are injected defects used to test the venue assertions. No live production
decoder defect is claimed. Fields in pending REST/websocket adapters are not
covered by this implemented-response audit.

Measured consuming control: exit 101 at
`consuming_native_collection_cardinality`, naming all seven native fields.
With that production mutant STILL ACTIVE, removing both source/native length
guards gives exit 0 (1 passed, 2 filtered). Restoring production and both guards
gives exit 0 (1 passed, 2 filtered). The independently pinned seven-field
comparison counter remains active in the bypass control. This is a targeted
guard-ablation result, not a claim that the entire types target passes the mutant.
A stricter preliminary ablation removing only the native guard also passed.
Logs: `.aimux/paradex/cardinality-consuming-both-guards.log` and
`.aimux/paradex/cardinality-consuming.log`.

Final scoped targets: `--test types --jobs 1` 3 passed / 0 failed;
`--test utilities --jobs 1` 1 passed / 0 failed. Four oracle SHA-256 values
rechecked unchanged; shared numeric-ingress source diff is empty.
Manifest statuses unchanged: 10 ported / 25 divergent / 118 pending.

## Review follow-up: remaining case loops

The first review found helper/account loops still lacked completed counters.
Helpers now require 33 positive-price, 32 fraction, 1 declared NaN refusal,
416 rounding and 32 interval comparisons. Account requires 19 completed cases;
the position-mode assertion remains a direct call outside the loop. Empty and
subset loop mutations for BOTH targets compile and fail respectively
`helper_completed_comparisons` and `account_completed_comparisons` (four exit
101 controls, four restored exit 0 runs). Existing oracle bytes are unchanged.
These are comparison-loop controls, not production-output mutations.

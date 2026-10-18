# BloFin declarations slice — 2026-10-13

Integration base: `a3d514bec82d6760d4e9e7a27186f992d175c36d`, merged down before
implementation. Scope: `blofin.types.ts` and its one manifest; the other six
BloFin manifest files remain untouched. All seven venue TypeScript files were
read end to end before this slice.

Python 3.14 `tomllib` over the seven venue manifests measured:

| State | Ported | Divergent | Pending | Not applicable | Total |
|---|---:|---:|---:|---:|---:|
| Before | 0 | 0 | 125 | 0 | 125 |
| This slice | 14 | 6 | 105 | 0 | 125 |

The 14 constants execute the actual TypeScript module, with actual imported enum
modules. Own values and key/array order are compared. The six response aliases
have no runtime validator in TypeScript: the witness pins their live AST schema,
executes typed JSON.parse identity probes, and checks every Rust native field
before comparing serialized output. This is declaration/ingress evidence, not
execution of a REST or websocket consumer. Initial Rust declarations and direct
field assertions were scaffolded from the source schema; this is not an
independently sourced fixture corpus.

Measured corpus: 36 response cases, 503 boundary/positive probes, 76 fields in
12 envelope/data records, plus the net/long/short enum. Every unconstrained
string field gets a distinct path-labelled value. Unicode/empty strings and
all side variants have positive probes. Missing required fields, optional
omission versus null, wrong scalar types, positional records, escaped lone
surrogates and a 140-deep unknown field have explicit source/Rust observations.
Duplicate keys are checked on the decoded native code field. Numeric source
probes capture typeof and bits before JSON serialization. There are no numeric
response fields: the recursive, fail-closed AST schema census measures only
string, literal union, record and array nodes. The receive-window constant is
compared as binary64 bits. No NaN inputs exist in this slice's declared fields.

All six response rows are CHOSEN divergences for their documented representation
and schema boundaries. They do not claim TypeScript performs runtime validation.
The lone-surrogate declaration is cited by its canonical anchor; depth and
unknown-field projection have separate explanations and witnesses.

`tests/controls.py` executed five sequential mutations, required the named
assertion to fail at exit 101, restored the source, then required exit 0:

| Mutation | Named failed assertion |
|---|---|
| Receive window 20000 to 20001 | `constant_RECV_WINDOW` |
| Symmetric Serialize/Deserialize positionId/instId swap | `decoded_FetchPositionResponseData_position_id` |
| Remove interface heritage refusal | `heritage_control` |
| Remove map-only record wrappers | `record_boundary` |
| Accept null in the optional-field decoder | `record_boundary` |

The initial null mutation only caused an unused-function compile error and was
not counted as runtime proof. The replacement mutates the decoder body and
reaches the assertion. All five restored controls passed. The schema census
continues to refuse unsupported syntax rather than silently omit fields.

Final scoped gates (`tests/gates.py`), all exit 0:

- Package rustfmt check.
- `cargo test -p safe-cex-venue-blofin --locked --jobs 1 --all-targets`:
  1 live-source integration test passed, 0 failed, 0 ignored.
- Strict host clippy, all targets, locked, jobs 1.
- Strict wasm32-unknown-unknown clippy, all targets, locked, jobs 1.
- Canonical extractor restricted to the changed TypeScript types file:
  20 declarations, 0 drift, source-file/options/global diagnostics checked.
- Canonical Rust parity checker restricted to this crate and all seven BloFin
  manifests: 125 rows, 27 public Rust items, no exemptions.

The earlier-row audit covers all 20 mapped rows. Their outgoing paths terminate
at constant initializers/enum values or shared numeric capture, serde, and the
map-only decoder; they do not reach Store notifications, timers, transport or
signing. Incoming future consumers do not change that reachability scope. No
previously shared fixture was edited; the schema pin is new and venue-local.

Not exercised or implemented: REST reads/writes, authentication, lifecycle,
websockets, exchange registration, live exchange I/O, whole-project TypeScript
extraction, monorepo tests, or integration merge-up. Those 105 rows remain
pending. The new Cargo.lock edges reference four already resolved dependencies;
no dependency versions or workspace features changed.

Reproduce from this worktree with `SAFE_CEX_TYPESCRIPT_PATH` set to the installed
TypeScript package:

```
python3.14 sdk-rs/safe-cex-venue-blofin/tests/controls.py
python3.14 sdk-rs/safe-cex-venue-blofin/tests/gates.py
```

## Per-row production mutation supplement

The subsequent owner instruction requires one production mutation per mapped row.
`tests/row_controls.py` parses the manifest with real tomllib and requires an
exact match between its mutation inventory and all mapped types rows. Measured:
20 mutations failed named runtime assertions at exit 101, and all 20 restored
runs passed at exit 0. This supplements the five original defect-class controls.

Each mutation/restoration uses only `--test types --locked --jobs 1`, serially.
Each test process invokes Node once for the whole corpus, not once per case.
The 20 controls therefore used 40 sequential Cargo/test runs and 40 Node corpus
invocations. No Yarn entrypoint was used; exit 126 cannot satisfy the runner.
Compiler failures cannot satisfy it either: the named runtime panic is required.

| Manifest row | Named failed assertion | Restored exit |
|---|---|---:|
| `RECV_WINDOW` | `constant_RECV_WINDOW` | 0 |
| `BROKER_ID` | `constant_BROKER_ID` | 0 |
| `USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY` | `constant_USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY` | 0 |
| `BASE_URL` | `constant_BASE_URL` | 0 |
| `BASE_WS_URL` | `constant_BASE_WS_URL` | 0 |
| `ENDPOINTS` | `constant_ENDPOINTS` | 0 |
| `GRAFANA_ENDPOINTS_EVERY` | `constant_GRAFANA_ENDPOINTS_EVERY` | 0 |
| `GRAFANA_ENDPOINTS_PERIODIC` | `constant_GRAFANA_ENDPOINTS_PERIODIC` | 0 |
| `INTERVAL` | `constant_INTERVAL` | 0 |
| `ORDER_STATUS` | `constant_ORDER_STATUS` | 0 |
| `ORDER_TYPE` | `constant_ORDER_TYPE` | 0 |
| `REVERSE_ORDER_TYPE` | `constant_REVERSE_ORDER_TYPE` | 0 |
| `ORDER_SIDE` | `constant_ORDER_SIDE` | 0 |
| `POSITION_SIDE` | `constant_POSITION_SIDE` | 0 |
| `FETCH_POSITION_RESPONSE` | `decoded_FetchPositionResponse_code` | 0 |
| `POSITION_MODE_RESPONSE` | `decoded_PositionModeResponse_code` | 0 |
| `FETCH_MARGIN_MODE_RESPONSE` | `decoded_FetchMarginModeResponse_code` | 0 |
| `FETCH_BATCH_LEV_RESPONSE` | `decoded_FetchBatchLevResponse_code` | 0 |
| `ORDERS_RESPONSE` | `decoded_OrdersResponse_code` | 0 |
| `POSITIONS_RESPONSE` | `decoded_PositionsResponse_code` | 0 |

Constant mutations alter their own production declaration. Each response mutation
symmetrically swaps its code/msg serde names in production; the decoded code
assertion catches the mismatch even though a wire roundtrip would agree.
Production source bytes were restored after every control; this supplement
changes proof tooling and annotations, not response or constant behavior.

## Income utility supplement

The maintained `blofin.income.ts` module executes in the oracle with actual
imported enum modules. Measured corpus: 548 cases (33 numeric helper, 175 fills,
340 closed positions), three numeric-ingress paths per case, and three shared
UTF-8/depth boundary probes. Numbers are captured as binary64 bits before JSON;
NaN alone is canonicalized. Every native output field and callback trace is
compared; no output serde roundtrip substitutes for native-field checks.

| Source row | Production mutation | Named runtime assertion |
| --- | --- | --- |
| toNumber | finite check becomes not-NaN | income_to_number_native |
| mapBlofinFillToIncomeRecords | fee negation removed | income_fill_native |
| mapBlofinClosedPosition | contracts * price becomes contracts + price | income_closed_native |

`tests/income_controls.py` requires runtime exit 101 plus the named panic, then
restored exit 0. All three controls passed; six sequential Cargo invocations,
one Node corpus per invocation. The final controls were rerun after adding
exact error-message comparisons; earlier controls are not the final evidence.
Logs: `sdk-rs/target/blofin-income-controls/*.fail.log` and `*.restored.log`.
The income rows declare the ordinary JSON/typed resolver domain, shared UTF-8
restriction and bounded ingress depth; they do not claim arbitrary executable
host objects or mutating callback access to input rows. No shared feeder changed.

## Order-history utility supplement

Actual maintained source corpus: 597 cases (36 toNumber, 36 toOptionalNumber,
525 mapper), three numeric-ingress paths, three additional UTF-8/depth probes.
Four mapper cases observe inherited constructor/prototype identity before JSON
and require the declared Rust RepresentationError. All output fields and native
enum variants are inspected directly, including omitted extra shared-model fields.

| Source row/control | Production mutation | Named runtime assertion |
| --- | --- | --- |
| toOptionalNumber | zero exclusion becomes one exclusion | history_optional_native |
| toNumber | finite check becomes not-NaN | history_to_number_native |
| mapBlofinHistoricalOrder | market mapping becomes limit | history_order_native |
| inherited boundary | inherited table result becomes default | history_inherited_refusal |

Final control runner: `tests/history_controls.py`, eight sequential Cargo test
invocations (four runtime fail101 / four restored pass0), one Node process per
whole corpus. Logs: `sdk-rs/target/blofin-history-controls/`. Initial three controls
were rerun after adding the inherited-boundary mutation; final evidence includes
all four. No shared fixture was regenerated.

## Stateless exchange utility supplement

The source oracle evaluates actual maintained AST initializer nodes for three
top-level helpers and five stateless class fields, with original source constants
and genuine imported enums. This is source-execution evidence for those helpers,
not constructor or REST-consumer evidence. Measured corpus: 1,268 cases, each
through three numeric-ingress paths, plus three UTF-8/depth refusal probes.
Capture header measured architecture arm64, Node major 26, version v26.7.0.
All four behavioral oracles now emit those header fields, which Rust verifies
and logs before comparing outputs. Prior accepted sheets stay attached to their
original SHAs; additive headers do not retroactively change those artifacts.

| Source helper | Production mutation | Named runtime assertion |
| --- | --- | --- |
| getNormalizedInstType | uppercase becomes lowercase | utility_getNormalizedInstType_native |
| isBlofinLinearSwapMarket | linear becomes inverse | utility_isBlofinLinearSwapMarket_native |
| isKnownNonSwapInstrument | non-SWAP comparison reversed | utility_isKnownNonSwapInstrument_native |
| parsePositivePrice | zero becomes admitted | utility_parsePositivePrice_native |
| mapBlofinTriggerType | mark maps to last price | utility_mapBlofinTriggerType_native |
| toBlofinTriggerType | index price maps to mark | utility_toBlofinTriggerType_native |
| isAlreadyGoneCancelRow | code 1000 becomes 1001 | utility_isAlreadyGoneCancelRow_native |
| isAlgoOrder | trailing-stop match removed | utility_isAlgoOrder_native |

`tests/utility_controls.py` measured eight runtime fail101 / restored pass0 pairs
in sixteen sequential scoped Cargo runs. Each run invokes Node once for the entire
corpus. Logs: `sdk-rs/target/blofin-utility-controls/`. Earlier production modules
are unchanged by this slice; only their capture metadata/checks were extended.

## Store-backed symbol supplement

276 source/Rust comparisons use the actual TypeScript DefaultStore and production
Rust DefaultStore with three shared numeric-ingress paths for market inputs.
Cases include collision aliases, own entries shadowing inherited names, stock
inherited hits (undefined), fallback and Unicode. Source lone-surrogate acceptance
is observed by code unit before Rust refusal. Two replaced-prototype probes
explicitly witness the declared shared Store boundary. Captures identify arm64,
Node major 26, version v26.7.0.

| Source row | Production mutation | Named runtime assertion |
| --- | --- | --- |
| symbolToMarketId | return market.symbol instead of id | symbols_symbolToMarketId_native |
| marketIdToSymbol | return market.id instead of symbol | symbols_marketIdToSymbol_native |

`tests/symbols_controls.py` measured two runtime fail101 / restored pass0 pairs
in four sequential scoped Cargo runs, with one Node process per corpus.
Logs: `sdk-rs/target/blofin-symbols-controls/`. Cargo.lock adds one dependency edge
to existing safe-cex-store; no package versions or shared features changed.

The later integration merge made Store::update fallible. Symbol-fixture setup now
requires successful updates explicitly; it does not ignore notification errors.
The source methods are read-only and introduce no notifying production mutation.
The pre-merge sheet is not evidence for the updated integration head.

## Ticker formatter supplement

Manifest measurement: 14 ported / 23 divergent / 88 pending, 125 total.
The actual formatter initializer executes with actual roundUSD and DefaultStore:
220 cases, twelve inherited-output observations, two UTF-8/depth probes, three
Rust ingress paths. Capture header: arm64, Node 26 (v26.7.0). Native field and
retained-info comparisons precede JSON; only NaN is canonicalized.
Reentrant emitters change fallback Store state before return; throwing emitters
retain their original identity. A named observation records the owned snapshot
versus source Store-array alias boundary. No production Store writes are added.

| Control | Named runtime assertion |
| --- | --- |
| bidPrice changed to askPrice | ticker_native_fields |
| emitter error discarded | ticker_host_error_identity |
| retained capture removed | ticker_ingress_retained |

All three mutations failed at runtime exit 101 and restored at exit 0, in six
sequential scoped test invocations, one Node invocation per corpus. Logs reside
in sdk-rs/target/blofin-tickers-controls. The symbol oracle dependency resolution
now anchors beside the compiler installation for isolated CI; its two controls
were rerun after that change. No shared fixture or dependency version changed.

## Order mapper supplement

The actual mapOrders initializer, actual non-swap filter helpers, constants/enums,
safe-math and DefaultStore execute once per 1,543-case corpus. Rust compares every
native field and retained info bits through three ingress paths. Source capture
header remains arm64, Node major 26 (v26.7.0). Invalid required output fields are
observed before JSON; Rust refusal retains only the preceding valid-output clock
prefix. This includes a valid/invalid/valid input. Source input and sibling info
aliasing is observed explicitly, alongside Rust owned output separation.
Inherited stock markets preserve their source coercion/error order; they do not
fall through to missing-market skips. No shared fixture was regenerated.

| Production mutation | Named runtime assertion |
| --- | --- |
| amount multiply becomes subtract | orders_native_fields |
| clock called before invalid opening-side refusal | orders_refusal_clock_boundary |
| invalid side becomes Buy fallback | orders_typed_boundary |
| retained capture removed | orders_ingress_retained |

All four mutations failed at runtime exit 101 and restored at exit 0, eight
sequential scoped test invocations, one Node process per corpus. Logs are in
sdk-rs/target/blofin-orders-controls. Two initial sub-agent findings prompted
clock-boundary and info-alias witnesses before final acceptance. Existing
production utilities gain only a crate-visible retained-value constructor;
all earlier mapped consumers remain covered by their scoped suite.

## Position mapper supplement

The source initializer, filter helpers, enum/constants, safe-math, createPositionId
and DefaultStore execute in one Node process per 806-case corpus. Three ingress
paths compare native Position fields/info bits, warning traces and errors.
The callback can change Store state before mapping or throw its original error.
Numeric strict-equality probes distinguish 0/-0, 1/1.0 and integers that round to
the same binary64 number above 2^53; reference equality has separate object probes.
The initial reviewer gap in numeric equality was fixed before promotion.
Captures identify arm64 and Node major 26 (v26.7.0).

| Production mutation | Named runtime assertion |
| --- | --- |
| contracts multiply becomes subtract | positions_native_fields |
| numeric equality becomes structural Value equality | positions_warning_trace |
| reference equality becomes structural equality | positions_warning_trace |
| warning uses supported first row instead of original first row | positions_warning_trace |
| emitter error discarded | positions_emitter_identity |
| retained capture removed | positions_ingress_retained |

All six controls require runtime failure 101 and restored pass 0. The final
controls were rerun after consuming the existing POSITION_SIDE constant; twelve
sequential scoped Cargo runs, one Node invocation per whole corpus. Logs reside
in sdk-rs/target/blofin-positions-controls. No shared fixture was regenerated and
no production Store mutation or journal grading policy is introduced.

## Precision-axis swap audit

Audited production baseline: `973fae2365ebf431231def79a6fbae6262f435a4`.
Nine production modules were enumerated and inspected, including numeric helper
arguments and constant/Store data flow. Implemented precision consumers are two
`market.precision.price` reads in map_positions: notional and unrealized PnL.
There are zero amount-step reads and zero implemented placement/stop/trigger/
bracket payload formatters. The ticker roundUSD call uses its fixed rounding
rule, not either market precision axis. Income/history/order projection uses
parseFloat and shared arithmetic without passing a market step.

`tests/precision_controls.py` independently changes each production price-step
read to the existing market amount-step field. It runs the ORIGINAL positions
integration target and ORIGINAL 806-case live-source corpus, before any fixture
repair. Both compile and fail `positions_native_fields` at runtime exit 101;
both restore to exit 0:

| Site | First failing case | Correct native value | Mutated native value |
| --- | ---: | ---: | ---: |
| position notional | 90 | 6.75 | 6.83 |
| position unrealized PnL | 1 | 5.5 | 5.38 |

The original market has fractional distinct steps: price 0.25, amount 0.01.
Native fields are compared directly as binary64 bits before output serialization.
Four sequential `--test positions --locked --jobs 1` runs each invoked Node once
for the whole corpus; captures identify arm64 and Node 26 (v26.7.0). Logs are
`sdk-rs/target/blofin-precision-controls/*.fail.log` and `*.restored.log`.
No test fixture or production behavior changed. This audit establishes rejection
at the two implemented position consumers only. It does not establish swap
resistance for the pending limit/stop/trigger/bracket formatters; their future
corpora must include fractional values across distinct step pairs and both sides.
The previously accepted PR candidate remains pinned; this is a separate proof
supplement, not a relabeled full gate sheet for a newer SHA.

## Inline TP/SL fallback utility supplement

Actual-source initializer plus genuine enum corpus: 266 cases, three shared
Order ingress paths, six malformed/UTF-8/depth refusal probes and one successful
unknown-field projection probe. Capture header: arm64, Node 26 (v26.7.0).
Source original-order and fallback-info aliases are observed before JSON and
compared with Rust-owned copies. Every native Order/OrderExtra field is compared,
including original optional fields, numeric bits, callback trace and exact errors.
No shared fixture changed. Two witness-transport issues (lone-surrogate JSON and
deep observation JSON) were fixed by code-unit/admission observations before
running the final controls; these were not production behavior repairs.

| Production mutation | Named runtime assertion |
| --- | --- |
| TP reads SL source field | inline_take_profit_price |
| SL reads TP source field | inline_stop_loss_price |
| native quantity becomes remaining base quantity | inline_fallback_native_quantity |
| deduplication bypassed | inline_order_count |
| ECMAScript trim replaced by Rust trim | inline_order_count |

Five runtime fail101/restored-pass0 pairs used ten sequential scoped Cargo tests,
one Node invocation per whole corpus. Logs: sdk-rs/target/blofin-inline_tpsl-controls.
Source review prompted the explicit successful unknown-field projection probe;
shared typed ingress is not described solely as refusal. This helper introduces
no Store calls, grading policy or market price/amount-step reads. These price-field
substitution controls are semantic controls, not precision-axis swap controls.

## Batch position-side admission

The declared `net | long | short` response field admitted externally tagged
objects through ordinary Serde and its qualified decoder. Both the maintained
accepted-set native probe and the new repository regression failed before the
repair. String admission now precedes a private remote enum companion, preserving
the existing rename attributes, native variants and serialization.

`batch_position_side_accepts_only_declared_strings` checks every literal and
tagged form through text, owned, borrowed, qualified and shared ingress routes.
`python3 sdk-rs/safe-cex-venue-blofin/tests/batch_side_controls.py` removes only
the String frontdoor: the actual remote decoder then admits the tagged map and
the named assertion fails at exit 101. Exact source restoration passes. A fresh
Rust 1.99 compiler inventory and the corresponding production-native contract
also pass; copied-source guard removal fails that native contract and restoration
passes without changing real production bytes.

Package validation: 112 tests pass across 24 target summaries, with one existing
ignored test; strict host and WASM all-target Clippy, formatting and canonical
parity pass. No ledger, source corpus, manifest row or shared gate policy changes.

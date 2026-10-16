# Phemex cardinality audit

Scope: the five implemented witness targets (constants, types, utilities,
account, ticker_reads). This is not a claim about pending REST formatters or WS
handlers. Integration a09ea6489b20 (including ba6468e0076f) is merged down on the separate cardinality
branch; submitted account and ticker heads remain pinned. Python 3.11 tomllib
reports 14 ported / 6 divergent / 59 pending, with no manifest status changes.

## Measured baseline test gap

At 9cc990e72ad6a7661924bac5209c28ead9dd20a9, changing the production response
mapping to `wire.positions.into_iter().take(1).collect()` compiled and the
original types target passed. Its source cases contained one position each;
checking position zero could not distinguish a truncating decoder. This is a
measured survivor, not a never-blind claim about the old corpus.

The original five oracle files are byte-identical to that head. Supplements
execute in the same Node process as the original oracle; subprocesses per corpus
remain one. Types use the actual TypeScript compiler's field inventory and typed
projection, not a nonexistent TypeScript JSON decoder. The utility supplement
executes the actual formatSymbolOut method.

| Original oracle | Bytes | SHA256 |
| --- | ---: | --- |
| types_oracle.cjs | 5994 | 7238a04b468bba6b03255d3dd35d1576bbf6ff61523a482abded770f2679b1f9 |
| utilities_oracle.cjs | 5208 | f15f14f6e3d1ae947ae09250dea4e4a00bd6a90660a4a27d8d6182caa4dae13c |
| constants_oracle.cjs | 3121 | ea718e85bdfbe5a43cff16a349b512db54d4e0d1d26d7da65048b2915c08a3d3 |
| account_oracle.cjs | 3490 | 22b40457a96d8e73763eb257c179141c622c23d51ab08415cbc737682cc0fb56 |
| ticker_reads_oracle.cjs | 4133 | ff048df17d089f47b75c975e9a7416006000fe980d359d9f8a3ab811f913510c |

Captured original JSON on Node 26.7.0 / major 26 / arm64 / darwin:
types 593562 bytes, SHA256
8c42b73986ebb8327cfef49cbe701f54c2210690adcfafef079c8e4f8328a451;
utilities 96102 bytes, SHA256
9b90ca89ddd6280919538c940db777f8d2da2cd01551754f3857f1461a3209f3.
Only NaN is canonicalized; signed zero remains exact.

## Census and independent expectations

- Constants: 13 exports compared as complete values; eight lookup tables and
  144 completed property comparisons. The reviewer identified the source-driven
  lookup loop as a remaining bypass risk; an independent 144 total now follows
  all successful comparisons.
- Types: fixed 71 bare cases and 74 envelopes, each through three ingress paths.
  Three supplement cases have fixed 0/2/3 positions, totaling five source
  positions and 15 native positions over three ingress paths. Independent native
  length equality precedes a fixed-index walk. Assert 450 completed field-set
  comparisons, plus a fixed per-case row-comparison count; the existing
  unknown-field check adds one separate comparison.
  Each field-set comparison directly checks all 49 declared native fields.
- Utilities: fixed 722 numeric, eight symbol-input, 26 original symbol-output,
  two supplemental symbol-output, and six numeric-output cases: 764 comparisons.
  Source uppercase rows and unique mappings both equal 1580; an independent
  Unicode scalar traversal performs 1112064 comparisons. Supplements return
  arrays of length zero and three. Output comparisons compare the whole native
  value, including tails and types.
- Account: fixed 30 native calls, whole event-vector equality and direct native
  account fields or error identity; assert 30 completed case comparisons.
- Ticker: fixed 23 native calls, whole recursive payload and event-vector
  equality; assert 23 completed case comparisons. Added object keys cannot be
  ignored by an expected-key-only walk.

No implemented bracket/stop formatter exists; the earlier precision result
remains a negative census, not per-field mutation coverage of pending methods.

## Executable controls

`tests/cardinality_controls.py` defines 18 compiled negative controls, each requiring
exit 101, execution of the test, and a named assertion; it restores the exact
source bytes and requires a passing target after each control. Twelve controls
empty, truncate, or extend four native collection sites (response positions,
Grafana endpoints, utility symbol arrays, ticker objects). Four controls remove
type-supplement expected rows, including bypassed source-count checks. Two more
keep consuming position-loss mutations while disabling both native length checks.

`tests/source_cardinality_controls.py` injects empty/subset source collections
AFTER the unchanged source oracle finishes. Its normal and `--bypass` modes
cover 15 explicitly listed source groups. Bypass mode disables source-presence
and source-count guards while retaining independent native assertions and fixed
traversals. Compile errors are rejected as proof. `--new-only` selects the two
new numeric-case and lookup paths. Runtime failures and restoration passes are
reported from measured execution, not inferred from this census.

Exact-SHA gate results and review accompany the pushed SHA. Extractor failure
prevents a completely green CI run; integration has no branch protection. The
coordinator's merge freeze is policy, not a GitHub-required-check constraint.

## Consuming decode control

Production census finds one implemented typed collection decode:
`PositionsResponse::deserialize`'s `Vec<PhemexPosition>`. Its empty/subset
controls run AFTER `Wire::deserialize` has consumed the complete record and all
array elements, then collect `wire.positions.into_iter().take(0)` or `.take(1)`.
They return successful decoded responses with missing positions; serde has no
trailing input to reject. Measured failures are respectively
`PHEMEX-CARDINALITY baseline native positions` and
`PHEMEX-CARDINALITY expected/native positions`, both compiled test exit 101,
followed by restored passes. This proves consumed-input loss at this venue
response boundary. It is not a claim that we mutated shared Array::next_element_seed
or covered every shared decoder. Account/ticker callbacks accept already-parsed
JSON; further order, fill, candle and book decoders remain pending.

With each consuming mutant retained and BOTH native length checks disabled,
the target still compiles and exits 101 at the independent fixed native position
access: `PHEMEX-CARDINALITY fixed native baseline position` (empty) and
`PHEMEX-CARDINALITY fixed native supplemental position` (subset). Therefore the
length assertions are not the sole defense in this target. We do not claim that
removing them makes this fixed-index witness vacuous; measured execution rejects
that claim. Each control restores and passes.

Measured control sheet: 18 native/type controls, 30 guarded-source controls,
and 30 bypassed-source controls compiled, executed, and failed by name (78).
Restored targets passed after each native control and each source-group pair
(18 + 15 + 15 = 48). Failed compilation attempts were excluded from these
numbers. The consuming paired controls were run with --consuming-only; the
two additional guarded groups with --new-only after the initial 13 groups.

## Fourth-branch sweep at dbdf3d15dddc

Before adding rows, inspection of five Rust targets, their source oracles and
both 78-control runners found zero assertion-bearing gated closures and two
filtered runner selections without a pinned execution floor (`--consuming-only`
and `--new-only`). Both runners now pin the full inventory, selected inventory,
and completed executions. `control_selection_controls.py` executes the actual
runner setup/guards in-process: four inventories pass, and twelve empty/subset/
skipped-loop controls fail named selection or execution assertions; no Cargo or
Node subprocess is needed for those runner controls.

One adjacent conditional assertion block in ticker_reads lacked a hit count.
Disabling its raw-lexeme/reference predicate compiled and PASSED the original
23-case target at dbdf3d15dddc. This is a measured test gap, not a production bug.
The block now pins two executions. `ticker_guard_control.py` disables it, requires
compiled exit101 at `PHEMEX-GATED ticker raw lexeme comparison count`, restores,
and requires a passing target. The actual source corpus and all five original
oracle files remain unchanged.

The independent reviewer matched this census. Two numeric-field filters in the
type oracle produce 21 account and 49 position edge cases, protected jointly by
the fixed 71-case total; this is not a separate per-group execution-count claim.
Utility Number/fallback and NaN/non-NaN arms assert outcomes on both paths, but
have no separately pinned per-arm counts. They are not skipped assertion closures.

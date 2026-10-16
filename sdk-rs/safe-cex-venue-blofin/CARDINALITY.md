# Cardinality audit

Scope: the nine implemented BloFin oracle targets at base
`0985e7cacc3ea66ef3953fc69f87d466ae86a40f`. This is a targeted census and control
set, not coverage of the 85 pending manifest rows. No production mapping bytes
change. Journal grading remains outside this venue.

## Original observations

The census found zero zip comparisons. Collection targets compare complete
native vectors, but none of the nine targets pinned its source case census.
The orders alias and inline info-alias walks also derived their bounds from the
oracle. Four response-array witnesses and both ticker paths only exercised
singleton successful outputs.

Twenty production controls ran on the original targets: ten empty and ten
`take(1)` subset mutations. All ten empty controls failed at runtime. Six subset
controls survived: FetchPositionResponse.data, FetchBatchLevResponse.data,
OrdersResponse.data, PositionsResponse.data, ticker projection and stored ticker
fallback. Income records, mapped orders, mapped positions and inline TP/SL
rejected their subset controls. The original orders subset rejection was an
incidental index-out-of-bounds during alias access, not named assertion proof.
Every production mutation was restored and its
original target passed. These are measured false greens, not an inference from
an inventory.

Twenty-two source splice controls ran on the original targets: empty and partial
case arrays in all nine targets, plus empty/partial orders aliases and inline
infoAliases. Twenty survived. Only the empty orders and positions case arrays
failed, through pre-existing refusal checks. Both targets still accepted removal
of their final case. Source script bytes were restored after every control. The initial orders-alias
`splice(1)` experiment was a no-op (each list had at most one pair), so its
result is withdrawn. The replacement deletes one pair from one case, changing
434 total pairs to 433; it also passes the original target. The runner now
requires every source splice to change the serialized artifact.

Original corpus hashes, case counts and byte lengths are in
`tests/support/cardinality-original.json`: UTF-8 `JSON.stringify(cases)` in
original array order; arm64, Node 26/v26.7.0. Baseline captures and all failure /
restoration logs are under `sdk-rs/target/blofin-cardinality-*` in this worktree.

## Repair

`tests/support/cardinality.json` is a frozen, reviewed pin, not a live count
calculated from the artifact being compared. It pins total arrays, every nested
array length and per-case branch selectors, including the operation/type name.
This prevents loss of a subcorpus hidden by a different handler's larger output.
Do not refresh it simply to make a failure pass.

All case, probe and boundary walks use independently pinned bounds. Native
collection lengths are checked directly against the pin, separately from source
counts and existing complete-vector equality. Types field assertions visit every
pinned data element. Nested ownership and generated-field walks use fixed
per-case counts; explicit counters run after assertions and check the number of
comparisons actually completed. Case/ingress index coverage is checked separately
from the aggregate total, so duplicate visits cannot replace an omitted index.
Positions refusal count is 216; its 540 fetched-leverage fixture entries are
counted separately from native output comparisons.

Four types cases and two ticker cases are appended. The former decode distinct
second records through the existing actual-source typed identities; the latter
execute actual formatTickers for a two-output projection and a two-ticker Store
fallback. The original case arrays remain byte-identical, with zero fixture-value
repairs. Final case counts: types 40, income 548, history 597, utility 1268,
symbols 276, tickers 222, orders 1543, positions 806, inline TP/SL 266.

## Reproducible controls

- `cardinality_controls.py repaired`: empty, subset and extra native outputs at
  each of ten collection-return sites. Requires a compiling runtime failure at
  `BLOFIN-CARDINALITY native length`, exit 101, followed by restored exit 0.
- `cardinality_controls.py bypass`: the same controls with both source array
  counts and branch-selector guards removed. Native guards must reject
  independently. The support file is restored on exit.
- `cardinality_source_controls.py repaired`: source case/alias empty and subset
  mutations; requires `BLOFIN-CARDINALITY source count`, exit 101, restored 0.
- `cardinality_consuming_controls.py guarded`: temporarily drain the shared
  numeric-ingress production `Array::next_element_seed` completely before
  returning `None`, then run the nine venue targets. This is an injected fault,
  not a claim of a live shared decoder defect.
- `cardinality_consuming_controls.py no-length`: retain that consuming mutant
  while disabling the independent native-length assertion and all four legacy
  types `data_length` assertions, plus the positions and inline legacy length
  assertions. Fixed native row coverage and existing field /
  vector comparisons remain; the control records which named layer rejects.
- `cardinality_counter_controls.py`: shorten native ingress walks or skip nested
  assertion walks; requires `BLOFIN-CARDINALITY executed comparisons`, exit 101,
  restored 0.

Each test launches Node once for the whole corpus. All Cargo commands run
sequentially with `--jobs 1`. Exit 126 and compile failures are not runtime proof.
These controls exercise the seven reported shapes: truncation, vacuous output,
zero-equals-zero, oracle-derived bounds, extra native output, lost totals /
subcorpora, and consuming decoder loss. The four venue response decoder controls
also consume the complete wire array before dropping elements; they never rely
on a trailing-input error. Zero inspected zip sites does not establish immunity for future code.

Package readiness uses the normal scoped test, fmt, strict host clippy, strict
wasm clippy and scoped parity sheet. A scoped pass does not claim a whole-workspace
wasm build or a green global extractor. Integration freeze is the owner's policy,
not GitHub branch protection.

## Store notification inventory

Repeated owner-scoped inventory, iteration 1 after merging integration
`a09ea6489b2012e43dfca621621ae8e916d4d03c`: all ten production files, closed by
the module list in lib.rs, contain eight Store reads, zero notifying writes,
zero subscriptions and zero BaseExchange calls. The two generic fallible emitter
calls propagate unchanged via `?` (tickers) and `map_err(Emitter)?` (positions);
zero silenced Results. A sub-agent independently inspected this inventory.
This does not enumerate other venues or establish that the workspace compiles.

## Measured repaired controls

115 compiling mutations failed named runtime assertions at exit 101, followed by
115 restored passes at exit 0: 30 native-output controls, the same 30 with source
guards bypassed, 22 source splices, 15 executed-comparison controls, and 18
consuming-decoder controls (nine guarded, nine with length guards disabled).
The consuming no-length controls still fail at independently named row/field/alias
assertions; this demonstrates redundant detection, not necessity of either
length guard alone. Restored production passes with those guards still disabled.

The pin measures 5,566 cases and 16,618 case/ingress comparisons; tests assert the
per-target totals and index coverage rather than relying on this printed sum.
Types also execute 503 probes, six five-case boundary walks and 26 native data-row
field-check blocks. Original oracle files outside types/tickers are unchanged.
One no-op source control and one non-compiling bypass attempt were excluded from
evidence, corrected and rerun. No trailing-input errors are counted as consuming
mutation evidence.

# Accepted decision contracts

This batch reviews eight function owners from the original native/default Aster
census. Each contract pins its actual compiler API, adapts the typed arguments
and calls the production function. All eight use executed decoder contracts;
there are no classification-only permissions or new production JSON APIs.

The 98 expected observations were authored from the maintained TypeScript bodies
before executing the Rust probes. `scripts/accepted-decision-source.cjs` extracts
the unchanged selected class members and top-level error helper, along with the
actual shared enum declarations, then checks those fixed observations against
their source execution. The two asynchronous exchange methods use fixture
transport/store boundaries to exercise their response/market decisions. They do
not establish HTTP, logging or full exchange construction coverage.

| Owner | Reviewed behavior and native observation |
| --- | --- |
| `account_helpers::parse_finite_number` | Number/string admission, finite check, whitespace and container refusal; actual binary64 bits include negative zero. |
| `account_helpers::get_order_update_time` | `T || E` precedes Number conversion; strictly positive finite time or lazy clock fallback. Both result bits and clock invocation count are observed, including coercion failure. |
| `account_helpers::get_position_side` | BOTH requires positive amount, LONG is exact, otherwise short. Actual native enum variants are matched, including zero/NaN and null-property error. |
| `exchange_helpers::is_aster_dex_unknown_order_error` | Error identity chooses message; other inputs use String conversion. Exact code spelling and case-insensitive message text, object-message rejection and coercion failure are retained. |
| `exchange_helpers::is_algo_order` | All six typed OrderType variants and five raw conditional strings are checked, with null, missing, numeric and incorrect-case raw types. |
| `exchange_helpers::reverse_map_time_in_force` | Empty typed string falls back to GTC; whitespace and unknown strings survive unchanged. |
| `exchange_helpers::parse_position_mode_response` | Only booleans and exact lowercase boolean strings pass; undefined/null errors and JSON.stringify failure text remain distinct. |
| `exchange_helpers::fetch_symbol_max_leverage` | First matching symbol wins; zero, negative zero and NaN default to ten, while negative finite values and infinities survive. Missing symbol rejects. Actual result bits are inspected. |

The adapters construct native OrderType arguments by matching each reviewed
fixture tag. Returned PositionSide values are inspected by native variant match,
not serialized round trips. Numeric tokens for NaN, infinities and negative zero
construct actual native f64 arguments for the pure leverage projection; the TS
adapter constructs the corresponding Number values. These tokens are test input
adapters and do not widen a production ingress.

`scripts/accepted-decision-contracts.mjs` checks the source expectations before
fresh native/default rustdoc discovery and canonical `runProbes`, with one Cargo
job. It requires exactly the eight selected function declarations, their exact
contract APIs and eight runtime receipts with no classification-only result.
It uses the maintained `loadContracts()` and per-crate `loadPolicy()` readers.

`scripts/accepted-decision-controls.py` compiles five isolated production faults:
loss of the negative-zero sign, admission of zero event time, classification of
zero position amount as long, refusal of the exact false string, and retention
of zero leverage instead of the fallback. Each must fail exactly its named
native owner with a function-output assertion. Compiler failures do not count.
The script restores exact source bytes after every fault and requires all eight
owners to pass after each restoration. Raw receipts remain outside the checkout.

The measured baseline and all five restorations passed eight runtime owners,
with zero failed owners and zero classification-only receipts. Each of the five
compiled faults produced exactly one failed owner at the expected behavioral
assertion. In particular, the negative-zero fault observed `0000000000000000`
instead of `8000000000000000`, and accepting zero event time changed both the
result bits and the lazy clock count from one to zero. The fixed 98 TS
expectations passed before every native run.

After integrating master `b5a2696998`, the native gate passed again with eight
runtime receipts and no failures/classifications. All 38 master contract rows,
including the eight alias rows, remain unchanged beside these eight additions.
The frozen dependency install, script syntax and whitespace checks passed;
`parity:check` passed 7,459 rows and 17,206 public Rust items with zero findings.

These are bounded function witnesses under the canonical
[string](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[typed output](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary) and
[numeric](../parity/MANIFEST-SPEC.md#numeric-ingress-defect) boundaries. They do
not classify records, private/local decoders, other owners, aliases or the
feature/WASM matrix. With the prior thirty owner contracts, these eight leave
146 original Aster owners for further review. Queue ddi2il-32 remains open.

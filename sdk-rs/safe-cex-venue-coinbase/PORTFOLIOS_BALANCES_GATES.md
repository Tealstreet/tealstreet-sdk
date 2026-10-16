# Coinbase portfolio and balance slice

Ten pending rows become ported: seven base exchange functions and the spot probe predicate, selector and supported-types function. Full exchange classes and HTTP/runtime construction stay pending. Authoring parent: `079b57e48d05cb74a0139613eb57f6253c3364ed`; no unrelated unreviewed ancestor was used.

## Measured gates

Code gate SHA: `cd9e7136eccf324fed5d28d8f6891f770e113e4e`. Each command exit is recorded in `PORTFOLIOS_BALANCES_GATE_RESULTS.json` with its actual output.

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2`: **0**.
- Scoped `cargo test` for `--lib --test portfolios_balances --test decoder_boundaries --test order_create --jobs 2`: **0**, 25 tests (9 library, 4 new native, 1 decoder inventory, 11 order-create). Oracle env: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**; output says “Would update 0 manifests; 0 added, 0 orphaned symbols.” Writer was run after manifest changes.
- `node sdk-rs/safe-cex-venue-coinbase/scripts/scoped-parity.cjs`: **0**; 426 Coinbase rows / 595 public native items; invention and status gates passed.

## Discrimination

Controls were measured at `4e5b60b1d0ff80b818a992fbad4449c59050419a`; scoped source/tests diff versus the code gate SHA exited **0**. Every mutation compiled, the named test `portfolio_balance_live_native_corpus` failed **101** at the assertion below, restoration passed **0**. Enumeration stopped after control 8; no survivors.

| Family / mutation | Named native assertion | Broken / restored |
| --- | --- | --- |
| list-error-arguments | `list-error: native error argument count` | 101 / 0 |
| base-input-order | `base-first-input` | 101 / 0 |
| spot-probe-predicate | `status-429: native probe predicate` | 101 / 0 |
| base-support | `base-public: native supported types` | 101 / 0 |
| spot-support | `spot-public-intx: native supported types` | 101 / 0 |
| balance-arithmetic | `cfm-prefix: native number bits` | 101 / 0 |
| spot-currency-arithmetic | `spot-keys-prototype: native number bits` | 101 / 0 |
| balance-composition | `balance-cfm-spot: balance currency presence` | 101 / 0 |

The initial second control was caught by `assert_portfolio` on the native name (`b` versus independently sourced `a`), before the runner’s anticipated type assertion. That was a matcher error, not a surviving mutation; the matcher was corrected and the eight-control run restarted.

## Scope and census

Native typed portfolio DTOs and declared numeric crypto-position fields; inert plain JSON responses/errors; original thrown roots retained with Rc. Native assertions precede any wire work (this slice has no encode comparison). Tests check f64 bits/NaN, CFM mutation and selected roots, plain subtraction/addition, concurrent wait-all with reverse completion, sequential hot spot probes, callbacks mutating list/UUID during captured-length iteration, ordinary-object key ordering and __proto__ assignment, cached roots, own undefined currencies and shallow extra aliases. Callable getters/hooks, sparse/exotic response DTOs, opaque errors and full class assembly are outside this mapping. No shared type field or shared trait was changed.

Fixture hash: `1bc2ff2a0dfbc6c7234bc561622e13408a77fcceca3b8378e90347c50dacd55b`; independent content pin and 47-case floor. The live oracle extracts all ten actual source bodies. Three additional native scheduling/receiver tests and two live-source controlled scenarios are floors, not exhaustive enumeration. Executor stops at first quiescence or fails at iteration1000; settled inspection stops after two polls.

Python3.11 `tomllib` census stops after 16 Coinbase manifests / 426 rows. At the authoring parent: 341 ported / 17 divergent / 68 pending; this slice: **351 / 17 / 58**, **10 pending→ported, 0 divergent**. Every moved row’s committed file and actual public function were independently verified; scoped parity-check additionally validates the mapping. Final integration-relative census and merge preservation will be attached separately.

Static sub-agent review found and corrected the missing/null reduce-property error and external-callback borrow. Final review found no blockers within the declared boundary; it ran no gates.

Not run: full Coinbase test suite, workspace builds/tests, host clippy, wasm clippy, CI, live HTTP/websocket. Precommit diagnostics and exact exits are retained in the JSON sheet; they are not claimed as gates over a later commit.

## Merge-down gates

Merged actual integration `e20df553203cb460fb7b4f46cf8dd65d9c589c3a` at `ec599eb7f3c01bc831c0fd44ee781867c694cafc`. Only exemption-file conflict: retained all 695 integration entries plus all 16 new native Coinbase helpers. Own source/tests diff against the code gate SHA exited **0**. Oracle/control scripts diff against the control SHA exited **0**. Numeric/types/store/transport/crypto/cache source diff exited **0**. Shared exchange source and lock changed, so scoped check, all 25 affected tests, extractor and scoped mapping were rerun at this merge SHA: **all exit 0**, extractor output zero updates. Eight source controls carry across unchanged inputs; they were not rerun after merging. See `PORTFOLIOS_BALANCES_MERGE_GATE_RESULTS.json`. Final tomllib census is attached to this actual merge SHA, compared directly to the integration SHA: **10 pending→ported, 0 divergent; 351/17/58**.

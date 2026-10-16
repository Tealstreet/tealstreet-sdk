# Final pending Coinbase declaration slice

**Moved 7 rows: 3 pending -> ported, 4 pending -> divergent/FINDING.** Independent Python 3.11 tomllib census against actual reviewed parent `d1652df92999c41782adbd5be274485279db548b`: **402 ported / 17 divergent / 7 pending -> 405 / 21 / 0**. Enumeration stopped after all sixteen Coinbase manifests / 426 declared rows. Every newly off-pending target is a real committed native fn/struct, independently inspected and validated by the unchanged scoped AST checker. [Exact census](COMPLETION_CENSUS.json).

The implementation and baseline gates belong to **`6fa3b033ed353ce2e9b1e9a6155819d1631acfe5`** on fresh branch `feat/rust-coinbase-completion`. The source parent is the reviewed integration above; no unrelated feature-branch ancestry was introduced. New implementation remains submitted feature work until reviewed/merged. Documentation commits preserve the measured source; they are not substituted for the tested SHA.

Ported rows: `createAPI`, `CoinbaseExchange.constructor`, `CoinbaseExchange.updateOrder`. Four genuine native class targets are explicitly divergent/FINDING; missing callback/state composition is named in each manifest row and [COMPLETION_FINDINGS.md](COMPLETION_FINDINGS.md). **Zero pending is not full-class or whole-venue parity.**

## Completed baseline gates at the implementation SHA

Commands, measured exits, complete output, elapsed times and source/test/script SHA256 fingerprints: [COMPLETION_GATES.json](COMPLETION_GATES.json).

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2`: **0**.
- Selected integration `cargo test` targets `completion`, `exchange_lifecycle`, `portfolios_balances`, `ticker_position_reads`, `public_stream_dispatch`, `private_stream_dispatch`, jobs 2: **0**, **25 passed**.
- Scoped library filter `private_order_snapshot::tests::`, jobs 2: **0**, **5 passed**; other library tests were filtered out.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: measured process exit **0**, output explicitly **Would update 0 manifests; 0 added, 0 orphaned symbols.** Yarn output, not merely the shim exit, is the gate. Writer was run after metadata edits.
- Unchanged scoped AST validator through `scripts/scoped-parity.cjs`: **0**, **426 rows / 948 public Rust items**, invention and status gates passed. This is a declaration/mapping gate, not behavior proof.

One Cargo process at a time. Oracle tests set `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. Actual constructor/data-field statements, API body and edit body are read from maintained TypeScript. The content-pinned input floor is **44 edit + 16 API + 10 class construction cases = 70 cases**; enumeration stopped there. No exhaustive input claim. Native payload fields, UTF16/native number values, own undefined slots, aliases, state and errors are asserted before serializer output equality. Class function-valued fields are separately indexed and omitted from the constructor oracle; full class wiring remains divergent.

Static sub-agent review checked method parity and class status honesty; it ran no gates. Its concrete findings (borrow retained into emitter, empty proxy coercion, serializer assertion order, source exception identity projection) were fixed before the measured commit. A preparation test had failed the Grafana alias identity assertion while exports were constants; exports now use stable static storage and the measured constructor test passes. Preparation attempts are not claimed as commit-bound gates or deliberate mutation controls.

## Mutation proof

[COMPLETION_CONTROLS.json](COMPLETION_CONTROLS.json) records compiled mutations, named failing assertions, restores, commands, logs, SHA and source hashes. Enumeration stops after nine explicit controls: API retry/header branch, amended price/amount precision, constructor rate, exchange shared selected slot, public heartbeat initial count, private initial attempt, and spot support field. Class controls prove real owned initial state/projections; they do not prove missing dynamic coupling.

All nine controls measured negative **101** at their named assertion and restored **0**. **No survivors among these nine controls**. Source/tests restored to the measured SHA with scoped git diff exit **0**. Controls belong to the same implementation SHA; neither their count nor their coverage is inferred from the output artifact.

## Not run

Workspace compilation, full Coinbase test suite, full monorepo suites, host clippy, wasm compilation/clippy, live venue/network tests, exhaustive mutation enumeration, and complete dynamic class integration proof. No shared type/trait was changed. No CI rerun, PR opening or integration merge was performed.

## Merge-down receipt

Merged actual integration **`2691078b11e3bff6d4541ede634a3ed1e54627ab`** at **`7450521f5012d56e3f36f663f51f74a22501e6ef`**. Only conflict was `sdk-rs/parity/exempt.toml`; both sets of unique entries were retained. No CLAUDE.md conflict occurred. Source/tests diff from gate SHA **0**; scripts/Coinbase manifests **0**; inspected compilation dependencies **0**; TypeScript oracle and checker inputs **0**. Exact commands and refreshed outputs: [COMPLETION_MERGE_RESULTS.json](COMPLETION_MERGE_RESULTS.json).

Extractor check re-executed at the merge SHA: **0**, zero proposed changes. Scoped AST mapping re-executed: **0**, 426 rows / 948 items. Cargo, runtime tests and mutations were **not rerun after merge**; their measured exits remain attached to `6fa3b033ed353ce2e9b1e9a6155819d1631acfe5` and the preservation receipt.

True gain against the actual integrated parent remains **3 pending -> ported, 4 -> divergent/FINDING**; final triple **405 / 21 / 0**, stopping at the same sixteen manifests / 426 rows. [Final committed tomllib census](COMPLETION_FINAL_CENSUS.json). Four class integration findings remain explicit.

# Coinbase ticker and position reads

Nine pending→ported rows: ticker dispatch, spot/CFM/INTX/V3 readers, position dispatch, CFM/INTX readers and zero-position completion. Authoring parent `845cd310a963e53fc50dd9180a152e034262b6b4`; no unrelated unreviewed feature ancestor. Full exchange assembly stays pending.

## Gates at committed code

Gate SHA `578f959d64893bc3b9b59f6a0511d5a11544bbec`; exact commands, exits and output in `TICKER_POSITION_GATE_RESULTS.json`.

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2`: **0**.
- Scoped `cargo test` with `--lib --test ticker_position_reads --test decoder_boundaries --test portfolios_balances --test spot_cfm_tickers --jobs 2`: **0**, 19 tests (9 library, 4 new native, 1 decoder inventory, 4 portfolio/balance, 1 existing spot/CFM formatter). Oracle environment `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**, text reports zero manifests updated / zero added / zero orphaned. Writer ran after manifest updates.
- `node sdk-rs/safe-cex-venue-coinbase/scripts/scoped-parity.cjs`: **0**, 426 rows / 624 public native items; invention and status gates passed.

## Controls

Controls at `5b38a702f06a49ef18d8633e1364af02f06a46a9`; source/tests diff from the code gate SHA exited **0**. Each compiled mutation failed the named native test **101** at the assertion below, then restored and passed **0**. Enumeration stopped after control10; no survivors.

| Mutation | Native assertion | Broken/restored |
| --- | --- | --- |
| ticker-fallback-root | `fetchTickersSpot-fallback: cached ticker array identity` | 101/0 |
| cfm-ticker-query | `fetchTickersCfm-normal: native value` | 101/0 |
| intx-funding-interval | `intx-normal.fundingTime: decoded native number bits` | 101/0 |
| v3-percentage | `v3-normal.percentage: decoded native number bits` | 101/0 |
| cfm-contract-conversion | `cfm-normal.contracts: decoded native number bits` | 101/0 |
| intx-unrealized-pnl | `positions-intx-normal.unrealizedPnl: decoded native number bits` | 101/0 |
| missing-position-side | `positions-fill-missing: position side` | 101/0 |
| concurrent-dispatch-order | `ticker reverse completion preserves input order` | 101/0 |
| cfm-live-response-list | `CFM live for-of sees appended response item` | 101/0 |
| cfm-callback-id | `CFM rereads ID after market lookup callback` | 101/0 |

## Scope and proof

Declared product/instrument/position DTO arrays, typed scalar Market/Ticker/Position fields and inert plain JSON error records. Native original response arrays and items retain identity; rejected INTX responses supply original item-array projection. Native Store/HTTP/logging/supported-type and pure timezone adapters preserve source operation boundaries. Date parsing uses the existing native JS date helper with an explicit timezone conversion; this corpus uses UTC. No shared type field or shared ExchangeRuntime method was added. Full class/HTTP parser construction, sparse/exotic DTOs, getters/callable hooks, opaque host exceptions and raw ingress are outside this slice.

All scalar fields/enums are asserted directly from decoded native values; retained info, cached arrays/items and rejected error roots use direct Rc assertions. Native request/emission Option<Value> arguments are checked before diagnostic projections, preserving undefined/null. There is no decode/re-encode equality. Distinct SPOT/CFM outputs discriminate reverse completion. CFM market lookup can mutate product_id and append a response item; source and native witnesses assert reread output/diagnostic IDs, live iteration and original info roots.

Fixture content pin `35456220b6a8ca0e71734412ad7171298958bfc7fcb0d32f2aff27dca1e5fb56`, independent41-case floor. Live AST oracle extracts nine actual source bodies plus existing mapped formatter/helpers. Three additional native controlled tests and five live-source scenarios are enumeration floors. Microtasks stop at first quiescence or fail at iteration1000; settled inspection stops after two polls.

Python3.11 tomllib census stops after16 Coinbase manifests /426 rows: authoring base351 ported /17 divergent /58 pending; this slice **360 /17 /49**, **9 pending→ported, 0 divergent**. Every moved row was independently checked for its committed file and actual public Rust function, in addition to scoped mapping. Final integration-relative census will be recorded after merge-down.

Static review caught and corrected CFM product-ID reread, non-discriminating empty reverse-order output, ignored error identity and array retention; final review reports no blockers in the declared boundary. Reviewer exits are not Cargo/test/extractor claims.

Not run: full Coinbase suite, workspace builds/tests, host clippy, wasm clippy/build, CI, live HTTP/websocket. Exact precommit diagnostics are recorded separately from gates; older gate exits do not cover the live-array correction.

## Integration merge preservation

Actual integration `3a957718fe445a833bb10a464e16e7cc6c585276` merged at `10f41a57404712e417c48b40bcf9234558d12f3d`. Exemption conflict resolved preserving all776 integration entries and all20 new Coinbase helpers. Source/tests diff versus578f959d64: **0**; oracle/control scripts versus5b38a702f0: **0**; dependency sources/manifests, Cargo lock and live TS oracle inputs: **0**. Thus scoped check19tests and10 controls carry across unchanged inputs; they were not rerun after merging. Merged extractor and scoped mapping remeasured at this merge SHA: **0/0**, extractor text zero updates, mapping426 rows/624 public items. Integration-relative tomllib census: **9 pending→ported, 0 divergent;360/17/49**. See tracked merge receipts and census.

Final merge: integration `6a15df74a80721009ec4b07ea1c4236c1d3242e7` at `7dc77b9b6dd08420d3ec5ea6f8cf88d78c1c1e3e`. Additional incoming work was Lighter/Weex only. Full scoped code/dependency/live-source diff against code gate **0**; oracle/control scripts versus control SHA **0**. Extractor/mapping remeasured at final merge SHA **0/0**. Final census compared directly with this fetched integration: **9 pending→ported, 0 divergent;360/17/49**. Tracked receipt `TICKER_POSITION_FINAL_PRESERVATION.json` binds these exits to their actual SHAs; subsequent delivery commit changes only evidence.


## PR 7620 conflict repair

Merged requested integration `61102ead04c728512ff39ee6ae5cf952e82bf2ad` at `9e0f7a91888be312e18e40a8a46c7e56037fec67`. The sole conflict was `parity/exempt.toml`; all integration entries and all20 Coinbase helper exemptions were preserved. Scoped check, the19 selected tests, extractor and scoped mapping were rerun there: **0/0/0/0**.

Integration advanced to `2a24bb3c48b25e781167071443dd603b0d0d2b8e` while gates ran; merged at `2a7719e344e137ca50389306551d8506fdce90c4`. Coinbase source/tests/scripts, dependency code/manifests/lock and live oracle inputs versus the first repair merge: scoped diff **0**. Original code-gate source/tests diff and control-script diff: **0/0**. Scoped check, the19 selected tests, extractor and scoped mapping rerun at this final merge: **0/0/0/0**. Extractor text reports zero updates; mapping426 rows/624 public items. Exact commands, test output, census and preservation exits are tracked in `TICKER_POSITION_PR7620_REPAIR_GATES.json`.

Tomllib census against captured latest integration remains **351/17/58 → 360/17/49**, preserving the original **9 pending→ported** rows with verified real targets; this repair adds **0** new rows. Original mutation controls were not rerun; their unchanged-source evidence remains tied to their original SHA. Full Coinbase suite, workspace checks/tests, clippy, wasm, CI and live network gates were not run. Subsequent evidence commit changes only these receipt files.

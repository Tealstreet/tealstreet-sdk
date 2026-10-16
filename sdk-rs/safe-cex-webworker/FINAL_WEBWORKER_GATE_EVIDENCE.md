# Final WEBWORKER runtime gate evidence

Code/test gate SHA: `2845d44798183cd9c0607f832c834206baea1c0e`. Actual integration parent: `18ab7a69e82b01b4c16aefbb68cc7b758ce960c1`. Branch: `feat/rust-webworker-final-runtime`. The two implementation commits and gate snapshot are this same reviewed slice; no separate unreviewed feature branch was used as an ancestor. Later merge/extractor evidence is recorded separately below.

## Rows

Independent Python 3.12 `tomllib` census compares `(ts, kind)` keys and statuses in all 38 baseline WEBWORKER manifests with the current manifests. Expected baseline: 222 rows, 115 ported / 95 divergent / 12 pending. Result: 115 ported / 107 divergent / **0 pending**; exactly **0 pending->ported, 12 pending->divergent**, with identical symbol keys and no additions/removals. This is a manifest census, not complete runtime or browser parity proof. Each new row has its own concrete CHOSEN or MIXED boundary and a real Rust target.

## Gates at the code/test SHA

All cargo commands ran sequentially with jobs 2 and `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript` for tests.

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2`: exit **0**.
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test chaser_engine --test worker_bootstrap --test exchange_worker_runtime --test chaser_manager_dispatch --jobs 2`: exit **0**, independent expected total **16 tests**, four per named target. All four result footers say `4 passed; 0 failed`. Native identity/decoded fields/number bits are asserted before source-profile or wire comparisons.
- Extractor writer after manifest edits: exit **0**, inspected text `Updated 0 manifests; 0 added, 0 orphaned symbols.` The writer used `parseManifest/serializeManifest` edits. Final committed-manifest check is bound separately below; the initial working-tree check is not claimed as covering a different committed manifest snapshot.

## Discriminating controls

Each mutation compiled, failed the named native assertion with exit **101**, was restored, then passed the exact selected test with exit **0**. Expected control count is eight row families, independently chosen from three chaser families plus startup/routing/factory/event/module families. No surviving mutation was dropped.

| Family | Deliberate mutation | Named assertion | Broken | Restored |
|---|---|---|---:|---:|
| placement | `if instance.is_stopped() {                 chaser_manager::log("tryPlaceOrders early-exit -> if false {                 chaser_manager::log("tryPlaceOrders early-exit` | CHASER native stopped zero placements | 101 | 0 |
| cleanup_completion | `instance.set_current_order_ids(vec![]); -> let _ = &instance;` | CHASER native IDs cleared before cleanup cancellation | 101 | 0 |
| monitor | `if count < 2 { -> if count < 1 {` | CHASER native first missing remains active | 101 | 0 |
| startup | `30_000., -> 3_000.,` | BOOT native startup timeout milliseconds | 101 | 0 |
| routing | `i += 1 -> i += 2` | BOOT native live replacement dispatch | 101 | 0 |
| factories | `hosted_mode != Some(true) -> hosted_mode == Some(true)` | BOOT native hosted dependent constructor count | 101 | 0 |
| event_bridge | `op: StoreWorkerOp::Fill, -> op: StoreWorkerOp::Log,` | BOOT native direct fill bridge op | 101 | 0 |
| module_tables_session | `.update(decode_update(decode("store")?)?, false)?; -> .update(decode_update(json!({}))?, false)?;` | SESSION native store seeded inside venue start prefix | 101 | 0 |

The initial runner stopped with exit 1 after routing failed the correct native assertion because its expected label was mistyped. The control itself compiled and exited 101; its `finally` restored production source. The resumed exact routing test passed 0, and the record uses the actual `live replacement dispatch` label. This was an evidence-runner correction, not a mutation survivor. Exact commands/exits and mutation texts are tracked in `FINAL_WEBWORKER_GATE_RESULTS.json`; log paths there identify local scratch logs, not purported committed attachments.

## Proof floors and review

Chaser witnesses execute actual TypeScript manager, instance, store, math, price, unknown-outcome and enum bodies. Worker witnesses execute actual ExchangeWorker and complete store.webworker bodies. Venue I/O, processor/handler construction/invocation, timers, clock-start, perf and browser listener/proxy operations are explicit leaf fixtures. Native worker fixtures use real BaseExchange; source venue-disposal fixture does not reset its store. Native cancellation therefore independently expects one disposal reset frame, then proves the retained startup loser adds no startup snapshot; no native/source equality of disposal-frame counts is claimed.

Enumeration stops at four new chaser tests, four new worker tests and the two existing four-test regression targets. Fixed host/source settling loops stop at iteration **32**. Retry witness stops at **19 Binance / 9 Bybit** placements, timeout at **30,000ms**, buffer at **1,000** entries, bridge enumeration at **12** log events plus fill/reconnect/deactivate. Held reprice cancellation applies REST filled=1 before placing amount=2, collapses concurrent update work to one 50ms timer, and checks unavailable reconciliation stops replacement. Missing confirmations distinguish ticker/position mutations; scalar update IDs distinguish number/string/null. Seed and map removal assertions run inside venue startup/disposal callbacks; account and anonymous table entries and identities are exercised. Native-only reentrant log/options and initial demo-health prefix regressions are not represented as additional paired source scenarios.

Not run or proved: workspace compilation, host/wasm clippy, wasm builds, full crate or monorepo suites, CI, live exchanges, browser/Go integration, exhaustive Promise microtask schedules or thrown/proxy/sparse JS objects. Newer-start preservation/failure races, monitor prefix-error finally, cancellation-error branches, completion-listener throws, config/reinit/Telegram interleavings, and optional fill callable receiver/argument identity are beyond the selected proof floor. No shared type fields or shared SDK trait methods were changed.

Read-only sub-agent `/root/context_review` reviewed the three new production modules, tests and corrected manifests: no remaining blocking findings, real targets and no default-success delegates. The review caught and repaired reentrant-borrow/prefix issues and strengthened repricing, seed/disposal ordering, anonymous tables and scalar-ID coverage; it ran **no gates**. Manifest corrections distinguish session snapshot routing, typed Store callbacks, and Base JSON nonfinite-to-null/typed decode boundaries.


## Integration preservation and committed-manifest gate

Merged integration `d5c0e7d58a7f03c8163680335e7b632ebb43ec86` down into this branch with exit **0**, producing `dd78133355b5c1303b2af7d0fc9fe4366c72310e`; no conflicts. Exact scoped `git diff --exit-code 2845d44798183cd9c0607f832c834206baea1c0e dd78133355b5c1303b2af7d0fc9fe4366c72310e -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` exited **0**. The direct SDK dependency source trees and tested TS oracle input files are also byte-unchanged. Only unrelated OKX dependency-list entries changed in Cargo.lock, and the package script change adds Kucoin test targets; neither changes this scoped cargo/test/control input graph. Original cargo/control exits remain attached to their original SHA and are carried by byte preservation, not claimed as fresh reruns.

Fresh extractor at committed merge SHA `dd78133355b5c1303b2af7d0fc9fe4366c72310e`: exit **0**, inspected footer `Would update 0 manifests; 0 added, 0 orphaned symbols.` Fresh postmerge tomllib census still moves 12 rows, all divergent, with zero pending. Subsequent evidence-only commit leaves source, tests and manifests unchanged. No integration merge/PR creation or CI rerun was performed by this lane.

# Coinbase exchange lifecycle slice

Built from reviewed integration `d5a67407c1ec26a197285f86e6a544188966f561`. Code/test gate SHA: `db8bd6942e8769b6e7e74053925aaffa41bb34a5`.
This file records measurements against that SHA; a later evidence-only commit is not a new measured code gate.

## Rows moved

Python 3.11 tomllib census stopped after all 16 Coinbase manifests / 426 declared rows, with no other venue census: 9 pending -> ported, 0 -> divergent. Parent 377/17/32; slice 386/17/23 (ported/divergent/pending). Every moved row resolves to a committed file and an actual public Rust function. This is a mapped-body census and bounded proof, not complete exchange runtime proof.

| TypeScript row | Rust function in exchange_lifecycle.rs |
| --- | --- |
| CoinbaseExchange.dispose | `dispose` |
| CoinbaseExchange.getAccount | `get_account` |
| CoinbaseExchange.validateAccount | `validate_account` |
| CoinbaseExchange.start | `start` |
| CoinbaseExchange._startPrivate | `start_private` |
| CoinbaseExchange._startPublicDataOnly | `start_public_data_only` |
| CoinbaseExchange#schedulePrivateUserStreamWriteCheck | `schedule_private_user_stream_write_check` |
| CoinbaseExchange#reconcileOpenOrdersFromRest | `reconcile_open_orders_from_rest` |
| CoinbaseExchange#scheduleOpenOrdersRestReconciliation | `schedule_open_orders_rest_reconciliation` |

## Measured gates

| Gate at code/test SHA | Exit | Scope |
| --- | --- | --- |
| cargo check | 0 | safe-cex-venue-coinbase --all-targets --jobs 2 |
| cargo test | 0 | 28 tests: 9 lib, 1 decoder inventory, 5 lifecycle, 4 portfolios/balances, 5 private dispatch, 4 public dispatch; --jobs 2 |
| parity:extract:check | 0 | output: Would update 0 manifests; 0 added, 0 orphaned |
| scoped-parity.cjs | 0 | unchanged checker: 426 rows / 737 public Rust items |

The live endpoint-table binding was updated after the main gate run. All five lifecycle tests were remeasured at `850cc70101e976a36d77f17ba63421a0d3a12cc2` with exit 0. Production source and tests remain byte-unchanged from the main gate SHA; the other 23 tests and scoped check are carried from that SHA. Commands and full output are tracked in LIFECYCLE_GATE_RESULTS.json. Oracle environment is SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Writer ran after the manifest edit and normalized one manifest.

## Native proof and controls

Fixture content pin: 45 declared cases, SHA-256 `70d1b1afe51ade5f3fd82a8ccd153561894f42c23eb8bff0f8591d4303ba4b97`. Enumeration stops at case 45. Six additional live-source scenarios witness private hot startup, initial awaited store update, unawaited public store update, live account root, and both reentrant timer callbacks. Source checkpoint drains stop after 12 microtasks; native drains stop at 1000 jobs or earlier quiescence.

Native assertions compare account/validation fields, own-balance presence, call order, typed Frame UTF-16 strings and number bits, loaded flags, fetched/merged/selected root identity, timer identity and reentrant replacement. They inspect native values directly, with no Rust decode/re-encode gate or wire-assertion claim.

Compiled controls measured so far: 7/7. Each record contains the named failure, exact command, output, exit 101, restored exit 0, and restoration proof. Controls are reported only after both runs finish; no pending run is claimed passed. The grace runner expected the later equal-frame case, but the unchanged timestamp in the earlier write-grace case discriminated first. That instrumentation mismatch exited 1 despite a compiled named failure at 101 and restored pass at 0; both the original expected name and observed assertion are retained in JSON.

| Mutation | Test | Broken exit | Restored exit | Named assertion |
| --- | --- | --- | --- | --- |
| dispose-order | `lifecycle_live_native_corpus` | 101 | 0 | dispose-empty: native call order |
| account-user-id | `lifecycle_live_native_corpus` | 101 | 0 | account-cached: native account userId |
| validation-own-presence | `lifecycle_live_native_corpus` | 101 | 0 | validate-ok: native balance own-property presence |
| startup-unawaited-update | `public_update_is_unawaited_but_initial_market_update_is_awaited` | 101 | 0 | initial market update must be awaited |
| write-grace-equality | `lifecycle_live_native_corpus` | 101 | 0 | write-grace: independent source invocation count |
| reconciliation-update-root | `lifecycle_live_native_corpus` | 101 | 0 | reconcile-ok: update.fields.orders: source array length |
| reconciliation-late-clear | `timer_callback_clears_handle_before_reentrant_effect` | 101 | 0 | reconciliation clears handle before hot fetch effect |

No measured survivor among the completed controls. scripts/lifecycle-controls.py restores source in finally and runs Cargo sequentially.

## Boundaries and limitations

Imported HTTP, BaseExchange disposal, store operations, sockets and timer scheduling are host capabilities. The host drives hot Promise producers, returns ordinary loaded objects, shares the receiver fields with dependent method bindings, and invokes registered timer callbacks once and asynchronously. This slice ports nine field bodies; full exchange construction, tick and the remaining 23 rows stay pending. A typed selected portfolio has its declared string UUID; returned frames preserve native roots and JS number/UTF-16 fields. Exception values use the existing PortfolioFailure model.

Sub-agent review was read-only; all four recommendations were addressed before the final gate run. Historical candidate compilation/schema/runner failures are explicitly recorded in JSON and are not counted as mutation controls.

Not run: full Coinbase test suite, workspace compilation/tests, host or WASM clippy, WASM build, CI, or live network/exchange calls.

## Integration merge and final measurements

Merged reviewed integration `9fdb2a05ffe14554db00996c29ab70cb5a12db1c` into branch at `a1c0a42058410ac93c091c6246a7b3bfe7674969`. Sole conflict: parity/exempt.toml, retaining all 1019 integration exemptions plus 33 real lifecycle representation/capability helpers. No CLAUDE conflict or read. Actual-integration tomllib comparison still measures 9 pending -> ported, 0 -> divergent; 386/17/23. Enumeration stopped at 16 files / 426 rows; no other venue census.

At this merged SHA, scoped cargo check --all-targets --jobs 2 exited 0; all 28 selected Coinbase tests exited 0; extractor check exited 0 with Would update 0 manifests; scoped parity checker exited 0 with 426 rows / 737 public Rust items. These are new measurements at the merge SHA. Integration changed Cargo.lock only to add safe-cex-crypto to Bitget dependencies; direct Coinbase dependency source and manifests remain unchanged.

Scoped git diff --exit-code from db8bd6942e to this merge for crate src/tests: 0. Oracle/scripts and Coinbase TypeScript diff from 850cc70101: 0. Direct dependency source diff: 0. Seven controls were not rerun after merge; their recorded native source/test inputs are byte-identical and the merged positive test run passed. Full commands, outputs, preservation exits and actual-integration census are tracked in LIFECYCLE_MERGE_GATE_RESULTS.json. An evidence-only descendant preserves these measured functional inputs; it is not separately claimed as a new Cargo gate.


Final merge-down: integration `7e9e5c660472d985e231394357b25ed171b929f2` merged at `0c92aeb7d4cac5761f32b235a44d508534d04876`. Byte-diff from measured merge a1c0a42058 covering Coinbase src/tests/scripts, Coinbase TypeScript, crate/workspace manifests, Cargo.lock and direct dependency src/manifests/build scripts exited 0. Cargo was not rerun after this second merge; its measured scoped check/28-test exits at a1c0a42058 are carried. Extractor and scoped mapping were remeasured here, both exit 0; extractor output Would update 0 manifests, mapping 426 rows / 737 public Rust items. Actual-integration tomllib census remains 9 pending -> ported, 0 -> divergent, 386/17/23. Full receipt: LIFECYCLE_FINAL_GATE_RESULTS.json.

PR 7694 conflict repair: merged reviewed integration `157f83240a85280c25df395ba6a51225813fe9bf` at `4a60258470e564671c587db04bcdf9f14455f092`. Sole conflict parity/exempt.toml retained all 1023 integration exemptions plus 33 lifecycle helpers. Scoped cargo check --all-targets --jobs 2 and all 28 selected Coinbase tests were rerun at this exact merged SHA, both exit 0. Extractor check exited 0 with Would update 0 manifests; scoped mapping exited 0 (426 rows / 737 public Rust items). Diff from previous pushed 2d4e9a9684 for Coinbase src/tests/scripts/manifests and TypeScript exited 0. Tomllib census: repair moves 0 pending -> ported, 0 -> divergent; branch retains nine pending -> ported relative to actual integration; triple 386/17/23. Stopped after 16 manifests / 426 declared rows. Seven mutation controls were not rerun for the repair and remain attached to their prior byte-identical native inputs. Full measured exits and outputs: LIFECYCLE_PR7694_REPAIR_GATE_RESULTS.json. Evidence-only descendant does not claim a new Cargo gate.

Second PR 7694 repair after another merge race: integration `0e014c0aa040874ed83ad9f13d9ae617c57a6435` merged at `e87f60af4a3a326d22204c85688abe69eea9adb6`. All four gates were rerun at that merged SHA: scoped check 0, 28 selected Coinbase tests 0, extractor 0 (Would update 0 manifests), scoped mapping 0. Native Coinbase src/tests/scripts/manifests and TypeScript remain byte-unchanged from previous push 2d4e9a9684. At metadata commit `3952a44dc9a135c18f3e024f2ce643b381cce3b1`, 33 lifecycle exemptions were relocated beside existing Coinbase entries instead of the shared append tail; every parsed exemption record remains identical and scoped mapping was remeasured with exit 0 (426 rows / 737 public Rust items). No Cargo rerun was claimed at the metadata-only descendant. Repair still moves 0 rows; branch preserves the nine all-ported rows, triple 386/17/23, from tomllib after 16 files / 426 declared rows. Both repair attempts and their exact gate SHAs are retained in LIFECYCLE_PR7694_REPAIR_GATE_RESULTS.json.

# MEXC PR 7424 rescue

Measured code SHA: `5ebb991d39f25015f665be0f1e6a57237227255b`. Subsequent evidence commits change documentation only. Original rescued remote head: `373edc2deeab2a932c05f36346c39ddd06d5c246`. The specified integration parent `45ff4e3dd2941ae338dd783a6177e94d00eccce0` was merged at `c89ecf6269f0530b4b0613a5e2f040576ed785d4`; refreshed integration `54851c81b7c780ebfea3682883bd3a49e12b9d8f` was then merged into code SHA above. Local alias `rescue/mexc-7424-merge-down` pushes to existing remote `feat/rust-venue-mexc` because that branch is registered in another worktree.

## Movement and targets

Python 3.14 `tomllib` census across six MEXC manifests, stopped after `mexc.ws-public.ts.toml`: **0 pending→ported, 3 pending→divergent** relative to both specified and refreshed integration; **88 ported / 40 divergent / 57 pending**, 185 rows. The rescued branch originally had 89/39/57. One inherited ported row (`cancelOrders`) was corrected to divergent FINDING, not claimed as a new pending move. Every one of the 128 off-pending rows has a real Rust target; official scoped invention/status gates cover 137 mapped targets, 133 public Rust items and zero venue exemptions. This is a target census, not proof of every inherited row's behavior.

The only merge conflict was the exemption registry. Resolution preserves the exact union of existing declarations with their reasons; no venue exemption was introduced. The MEXC scoped checker reads the governed registry, filters foreign crate declarations, and uses the official parity checker against only MEXC's six manifests/source index.

## Findings and repaired gates

At initial merge `c89ecf6269`, scoped check exited 0 but the suite exited 101 in `mexc_mapper_owned_snapshot_vs_live_alias`: the oracle seeded 8/2 while the independent native expectations required 1/1. Restored those fixture seeds; did not lower the expectations. At `58ae9a845a9b9762ac94fc473851347c138f8ee6`, scoped check exited 0 and 45 unit, 3 alias and 8 API tests passed before the client suite failed (101) at `preimages 45`. The test recorder recognized only literal `old`, while the caller's fixture signature was `old-signature`. It now recognizes the original case-insensitive caller signature. Production signing was unchanged; exact source preimage and transmitted header assertions remain. Focused client rerun on the corrected working tree passed three tests (0); final committed gates below supersede that dirty-tree run.

`exchange.rs::MexcExchange::post_cancel_payload` recreates a message-only ExchangeError, prioritizing response.message; TypeScript propagates the original rejected error. New `mexc_cancel_transport_projection_finding` directly asserts source message `transport failure`, code `ERR_NETWORK`, original response, versus native Rust message `response detail` and absent code/response. Native order status remains Open. Consequently `cancelOrders` is divergent FINDING. Validation/account rows retain their actual response-detail findings, and notes now claim only the five exercised source cases. Successful validation balance free/used/total and cancellation OrderStatus/id are asserted on native values before any encoded comparison.

## Production mutation controls

Fixed floor three controls, stopped after `cancel-status`; no survivors. Each runs its named native source-oracle test with baseline exit 0, actual production mutation exit 101 and restored exit 0. `scripts/rescue-controls.py` restores the production source in a finally block and binds its summary to the code SHA.

| Control | Production change | Named failing assertion | Baseline / mutation / restored |
| --- | --- | --- | --- |
| validation-success | empty validation error → unexpected | MEXC account validation result | 0 / 101 / 0 |
| account-hash | digest[..12] → digest[..11] | MEXC account identity | 0 / 101 / 0 |
| cancel-status | store Canceled → Open | cancelOrders native status: | 0 / 101 / 0 |

## Review and limits

Native sub-agent review found the cancellation transport mismatch and overclaimed validation coverage; the final static review found no further actionable issue. It verified the registry union and concrete targets. Reviewer ran no gates; all gates below were run by the authoring rescue agent.

**Unreviewed ancestry warning:** the dead branch includes shared-core WIP commits `204da7be8d`, `157a3b3098`, and `51fd4fbe20` (owned timers/runtime/Store audit), beyond the three MEXC rows. A scoped MEXC check is not workspace compilation. The optional owned_tasks_tests subset does not establish all shared-core behavior. No full workspace/monorepo build, host clippy, wasm check/clippy, full core suite, live exchange call, CI rerun or PR creation was performed. The next pending-row slice branches independently from integration so it will not inherit this WIP.

## Exact gate results

All commands below measured at the exact code SHA above; raw outputs and command arrays are tracked in `evidence/rescue-7424/`.

| Gate | Exit | Observed scope |
| --- | --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --all-targets --jobs 2 --locked | 0 | MEXC and dependency compile |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --jobs 2 --locked -- --nocapture | 0 | 108 tests across 13 targets; zero doc tests |
| cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc -- --check | 0 | MEXC formatting |
| python3.14 sdk-rs/safe-cex-venue-mexc/scripts/scoped-parity.py | 0 | 185 rows, 133 public items, zero exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | Read text: 7341 symbols, 512 files, zero stale/added/orphaned |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-exchange --lib --jobs 2 --locked owned_tasks_tests -- --nocapture | 0 | **Zero matches, not a gate** |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-exchange --lib --jobs 2 --locked owned_tasks::tests:: -- --nocapture | 0 | Actual ten-test module; 71 other tests filtered |
| python3.14 sdk-rs/safe-cex-venue-mexc/scripts/rescue-controls.py | 0 | Three actual production controls, each 0/101/0 |

The corrected core filter passed all ten tests; its static file enumeration stops at `callback_reporter_sees_rearmed_interval_and_can_cancel_it`. No mutation-proof claim is made for that inherited core module. The first zero-match invocation is retained as a failed attempt to select a behavioral gate, despite process exit 0. Both source-oracle gates and controls used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

## Final merge preservation

Immediately before delivery, fetched integration again and merged `d7233c152349b3076a7469c481f69289589ec702` at `671661eed072090cf28fec1c6cb4fb4cf9e40628`. `git diff --exit-code 5ebb991d39f25015f665be0f1e6a57237227255b HEAD -- sdk-rs/safe-cex-venue-mexc/src sdk-rs/safe-cex-venue-mexc/tests` exited 0; extending it to all seven MEXC dependency crates and its six manifests also exited 0. Cargo.lock changed only the GRVT dependency list. The 108-test/three-control exits remain attached to 5ebb, with byte-unchanged input preservation established. Fresh scoped MEXC check and extractor check at 671661 each exited 0; extractor output again zero stale/added/orphaned. These two SHA-bound outputs are tracked in `evidence/rescue-7424/post-merge/`. No claim that the full MEXC tests or mutations were re-executed at 671661.

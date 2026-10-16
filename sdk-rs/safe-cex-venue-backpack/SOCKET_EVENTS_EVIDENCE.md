# Backpack inbound socket events

Initial measured code SHA: **`9924c59b21e4367000b5e05a4516e1cc8df14465`**.
Branch: `feat/rust-backpack-public-events`.
Actual parent: **`5755c1bbc8ab6088443ec0d9e1ab95f3895aee10`**, the user-specified integration head. Fresh branch; no prior unreviewed Backpack branch is an ancestor outside integration. Initial tracked worktree was clean, with no leftover mutation; the three Aimux briefing files stayed untracked.

Python 3.14 `tomllib` compared every Backpack manifest symbol with that parent: **8 pending → ported, 0 → divergent**. Parent census: 52 ported / 53 divergent / 42 pending. Code census: **60 ported / 53 divergent / 34 pending**, 147 rows. These are census claims, separate from proof.

The eight rows are both public/private `onMessage`, plus public `handleDepthUpdate`, `updateTickerFromDepth`, `handleTickerUpdate`, `handleTradeUpdate`, `handleBookTickerUpdate`, and `handleKlineUpdate`. The public ticker row includes native callback creation and supplied throttle installation with 1000ms leading/trailing parameters.

The method bodies use compatible native property/call operations. Parser, store, handler and scheduler operations are supplied by the runtime; full socket classes/adapters and arbitrary host prototypes are not claimed. No shared type or shared runtime trait was changed. All native event identities, callback sequences, UTF-16 fields/messages, optional patch fields and f64 bits are asserted directly. No decode/re-encode comparison supplies native evidence. There are no wire-equality assertions in this target.

## Gates measured at the code SHA

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2`: **exit 0**.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test socket_events --jobs 2`: **exit 0**, 4 passed / 0 failed, enumerating the 218 cases below.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit 0**; output explicitly reports 7341 symbols across 512 files, would update 0 manifests / add 0 / orphan 0.
- `git diff --check` after restoring all controls: **exit 0**, tracked diff empty.

The required manifest writer was run after manifest changes, before committing the measured code: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`, **exit 0**, output updated 0 manifests / added 0 / orphaned 0. No Rust-file arrays were hand-spelled.

Not run: workspace builds, host clippy, wasm build/clippy, full crate or monorepo suites, CI/retries, live exchange/network flows, or throttle-clock/scheduler timing tests. Throttle factory parameters and native callback behavior were measured with supplied factory operations; that is not proof of a complete scheduler implementation.

## Independent enumeration floor

The Node oracle executes the actual TypeScript socket classes and actual `packages/json-parse/src/jsonParse.ts`, with scoped runtime operations. Rust independently pins cardinalities and case inputs; expectations are not copied from the artifact's lengths.

| Family | Independent expectation | Last completed zero-based iteration |
| --- | --- | --- |
| Routing | 72 = two socket scopes × 36 independently listed packet/failure/accessor/disposal cases; malformed/falsy JSON, confirmations, numeric versus string pong codes, wrapped/flat data, exact/prefixed streams, non-string stream error, capture order | Public 35; private 35 (global 71) |
| Numeric updates | 110 = depth 41 + ticker 23 + trade 23 + book ticker 23; six asymmetric UTF-16 numeric input tuples, missing ticker/update failure and five lookup/field/logging failures; depth adds empty-side cases | Global 109; depth 40; each other method 22 |
| Handler delivery | 36 = depth 16 + kline 20; handler presence, failure boundaries, holes/removal, append outside initial length, captured-array replacement, symbol mutation, logging failures, throttle access, stream fallback and null guards | Depth 15; kline 19 (global 35) |

Total **218 finite cases**. This is a floor, not exhaustive JavaScript host behavior.

## Compiling discrimination controls

Each control was applied to the measured code tree, run serially with the oracle environment, restored byte-identically from saved source bytes, and rerun with the same named test:

`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test socket_events --jobs 2 <test> -- --exact`

All eight mutations **exited 101 from the named assertion**, and all eight restored runs **exited 0, one passed / zero failed**. No compile-error controls or surviving mutations are counted.

| Control iteration | Test | Mutation | Named assertion |
| --- | --- | --- | --- |
| 0 | socket_routing | Change pong interval 10000 to 9999 | ROUTING native pong interval |
| 1 | socket_routing | Resolve order handler after final payload getter | ROUTING native captured handler identity |
| 2 | ticker_updates | Round the shared parseFloat result | NUMERIC native decoded ask 0 |
| 3 | ticker_updates | Change ticker throttle delay 1000 to 999 | THROTTLE native ticker delay |
| 4 | depth_delivery | Resolve throttle factory after second symbol getter | DELIVERY native throttle symbol depth/15 |
| 5 | kline_delivery | Bypass return on null candle fields | DELIVERY native callback sequence kline/15 |
| 6 | socket_routing | Delete one routing record after construction | ROUTING independent corpus size, 72 expected / 71 actual |
| 7 | ticker_updates | Fill all 110 numeric records with the first record | EVENT independent case mode, numeric iteration 1: none versus independently expected missing |

Stopped after control iteration **7**. The controls cover the new method families and both concrete review findings; untried mutations are not declared caught.

Restored SHA-256 pins: `src/socket_events.rs` = `1c5b6ade9495f40aa195f9db28e9af29228183f559ac9f617cbfae6c812eead0`; `tests/socket_events_oracle.cjs` = `9a0eec144861cc3d7f52775defbb2b524c591cab705f2b08bd9361dbf44a84de`.

## Review findings and development failures

Read-only sub-agent review independently reproduced two real gaps using actual-source Node witnesses (reviewer reported **exit 0**): private order/position routing must capture its handler before the final `json.data` read; depth delivery must capture its throttle factory before the second `data.s` read. Both were corrected before the measured SHA, with new accessor cases that replace the live handler/payload or change the symbol. Native captured-reference and argument assertions, plus controls 1 and 4, distinguish the corrections. Follow-up review closed both findings and reported no new concrete defects within the declared operation scopes. The reviewer ran no Cargo; the follow-up review ran no gates.

An earlier uncommitted development Cargo run exited **101** on three unused test-context fields under deny-warnings. The fields were removed, without warning suppressions, and the scoped target passed. An earlier source-oracle development run exited **1** because the fixture's assertion message coerced an error even when the error identity matched, triggering its deliberate throwing formatter. The assertion message stopped coercing the error; the source oracle then exited **0**. Neither development failure is counted as a semantic mutation proof or a gate at the code SHA.

## Surviving callee-kind control and corrected gate

Initial merge-down fetched integration **`e20df553203cb460fb7b4f46cf8dd65d9c589c3a`**, producing **`565f0447cbf026f0e6e1536c717499c9c526582c`**. The required scoped `git diff --exit-code 9924c59b21e4367000b5e05a4516e1cc8df14465 HEAD -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests` exited **0**. TypeScript oracle inputs were unchanged; Cargo.lock and the transitive safe-cex-exchange dependency changed. At that merge SHA, separately measured scoped check **0**, socket_events target **0** (4 passed), and extractor **0** (would update/add/orphan 0).

**FINDING: a real compiling mutation survived at 565f0447cbf026f0e6e1536c717499c9c526582c.** At `src/socket_events.rs::message`, the public call site `dispatch_method(selected)` was changed to `dispatch_method(EventKind::Trade)`. The named `socket_routing` test exited **0**, one passed. After byte-identical restoration it again exited **0**, one passed. The mock dispatcher reused one identity for all handler kinds; invocation recorded the separate requested-kind metadata instead of the captured callee. That let the wrong method lookup masquerade as a correct dispatch. The survivor is retained here, not counted as a caught control or dropped from the record.

Corrected code SHA: **`344fb9c64cc6fc7525708d2b77d2289581e4b5c4`**. The duplicate kind argument was removed from the venue-local dispatch operation; invocation receives only the captured dispatcher and source arguments. The native fixture dispatcher now retains its captured kind and identity, and records the captured kind when invoked. Reference-view clones must retain JavaScript object identity; runtime providers supply those compatible views. No shared runtime trait or shared type changed. Read-only sub-agent review confirmed that the correction removes the masking path and found no new defect in the diff; it ran no gates.

At the corrected SHA, independently measured again:

- The same scoped Backpack all-targets check: **exit 0**.
- The same socket_events target/environment: **exit 0**, 4 passed / 0 failed, the same independently pinned 218 cases.
- The same extractor checker: **exit 0**, output 7341 symbols / 512 files, would update 0 / add 0 / orphan 0.
- `git diff --check` after all controls restored: **exit 0**, tracked diff empty.

All original controls **0–7** were rerun at this corrected SHA, with the same commands, named assertions and mutations in the table: **each mutation exited 101; each byte-identical restored run exited 0, one passed / zero failed**. The formerly surviving mutation was also rerun as control **8** at this SHA: forcing the Trade lookup now exited **101** on **`ROUTING native dispatched kind false/9`**, observing native Trade versus independently expected Depth. Its byte-identical restored run exited **0**, one passed / zero failed.

The corrected proof floor stops at control iteration **8**, nine caught controls at this SHA. Historical measurements above remain attached to their original SHAs; they are not relabeled as corrected-SHA measurements. The earlier survivor is resolved by a discriminating native assertion. Current restored source SHA-256 is `ad7b0eb4a895453df93255831d5a53cb4e96db7db59a7a0b8a2244829ea3f5c2`; the oracle SHA-256 remains `9a0eec144861cc3d7f52775defbb2b524c591cab705f2b08bd9361dbf44a84de`. Census remains **8 pending → ported, 0 → divergent; 60 ported / 53 divergent / 34 pending**. The unrun gates and finite-enumeration limits above remain unchanged.

## Pre-push preservation

Pre-push merge-down fetched integration **`9d7e0523cd16502a66901c3bf264e222bc5081ba`** and produced **`174403e17e02ed5e056f2563ff89be8db0f6c98a`**. Required preservation command:

`git diff --exit-code 344fb9c64cc6fc7525708d2b77d2289581e4b5c4 HEAD -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests`

Measured **exit 0**, byte-unchanged. Changed-path counts also remained zero for workspace Cargo.toml/Cargo.lock, this crate's Cargo.toml, safe-cex-num/types/crypto/exchange/transport directories, TypeScript Backpack sources/shared types, and the actual jsonParse source. The corrected-SHA Cargo/oracle/control exits therefore stand as measurements at that SHA; they were not rerun or relabeled at the later merge. Other venues' manifests changed, so a fresh extractor check at **174403e17e02ed5e056f2563ff89be8db0f6c98a** measured **exit 0**, output would update 0 / add 0 / orphan 0, 7341 symbols / 512 files. `git diff --check` measured **0**. Independent tomllib census still measures **8 pending → ported, 0 → divergent; 60 ported / 53 divergent / 34 pending**. The following evidence-only commit changes no measured source, tests, dependencies or manifests.

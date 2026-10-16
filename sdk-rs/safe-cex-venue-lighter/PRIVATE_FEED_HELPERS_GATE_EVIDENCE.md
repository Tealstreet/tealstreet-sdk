# Lighter private-feed helper bodies

Actual parent: `7d61bf63d82a52cb0da4731131e8542569cc820d`, the requested integration head. Fresh branch: `feat/rust-lighter-private-feed-helpers`. No unreviewed feature ancestor was added. Implementation and frozen gate SHA: `7aa2378520195d3da56de60f3fcd0ed046b0f3a5`.

## Eight pending rows moved

Independent Python 3.11 `tomllib` comparison against the parent exited 0: **8 pending → ported, 0 → divergent**. The unchanged 155 rows in my five owned manifests change `(ported, divergent, pending)` from **(101, 6, 48) to (109, 6, 40)**. Enumeration stops at these five manifests and eight status changes. Each changed row's public function and runtime struct target was checked against an actual declaration; zero missing targets.

The rows are `#normalizeOrderStatus`, `#isUserCancelledReason`, `#resolveExplicitlyClosedPositionUpdates`, `#scheduleThrottledTask`, `throttleFetchOrders`, `throttleFetchPositions`, `throttleFetchBalance`, and `#sendSubscribePayload` in `lighter.ws-private.ts`. All map to real bodies in `src/ws_private_feed_helpers.rs`, with `LighterPrivateFeedRuntime` providing required property, intrinsic, RPC, store, Promise and timer capabilities.

Normalization preserves nullish alias selection and UTF-16. Explicit closes require finite parsed zero, preserve original position/raw-info identities, copy the original position, and reset exactly the source fields. Throttles retain the live `scheduleThrottledTask` callee/receiver, cancellation, skip guards, RPC suspension, source disposal behavior and full updates. Orders check disposal after fetch; positions deliberately do not. Timestamp assignment captures its parent before the clock call. Subscription logging captures its receiver before channel evaluation, and sending captures its receiver before serialization.

The whole-class row remains pending: these bodies do not assemble the entire websocket class or reflection surface. This slice authors only the owned private-feed manifest and its venue module/export, fixture, oracle, mutation driver and sheet. It does not author `lighter.exchange.ts`, SDK source/manifests, or shared types/traits. Existing Promise/error records and shared numeric helpers are imported.

## Frozen gates

All cargo invocations are sequential with two jobs. `CARGO` means `/opt/homebrew/opt/rustup/bin/cargo`; `YARN` means `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`. Every native test invocation sets `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Measured SHA | Command | Observed result |
| --- | --- | --- |
| `7aa2378520195d3da56de60f3fcd0ed046b0f3a5` | `CARGO check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2` | exit 0; Finished dev profile |
| `7aa2378520195d3da56de60f3fcd0ed046b0f3a5` | `CARGO test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_private_feed_helpers` | exit 0; 4 passed, 0 failed, 0 ignored |
| `7aa2378520195d3da56de60f3fcd0ed046b0f3a5` | `YARN parity:extract:check` | exit 0; Would update 0 manifests, 0 added, 0 orphaned symbols |

After editing the manifest, the writer `YARN parity:extract` exited 0 before this commit: 1 updated manifest, 0 added, 0 orphaned. Output text was read. Precommit development exposed an oracle syntax error (exit 1) and fixture compilation errors (exit 101: private numeric helper, wrong enum variant); both were fixed before freezing. Neither is a mutation proof. A passing development fixture was strengthened with an early named timer-count assertion before the frozen run.

## Native evidence and limits

The oracle transpiles and executes the unmodified complete TypeScript private websocket module, the actual source domain enums and logging enum. Controlled base websocket, RPC/store, clock and timer operations supply imported dependencies. Independent fixed floors are **10 status + 8 cancellation + 16 closes + 6 subscriptions + 13 orders + 10 positions + 8 balance + 4 scheduler = 75 cases**. Three native test groups pin 34 normalization/close, 35 timer and 6 subscription cases. A fourth separately drops an observer and verifies the retained orders callback still finishes its full store update. Native draining stops after at most 100 callbacks; enumeration stops at this corpus, not arbitrary scheduler interleavings.

Actual native results are compared to decoded source values, preserving UTF-16, BigInt, binary64, arrays and object fields. Source identity receipts come from the original first returned result. Native original/new/info handles, RPC/store/log/scheduler receivers, timer cancellation handles, injected errors, outcomes, cleared fields, suspension, updates and log arguments precede subscription wire assertions. No native decode/re-encode comparison is used. Family counts are independent constants; expected native timer counts come from separately executed source timers, not native output.

Read-only sub-agent review found an initially omitted live scheduler lookup. The body now captures and invokes the actual `this.scheduleThrottledTask` callee; getter failure and override behavior are in the source/native corpus. Review of the five control anchors found no blockers and confirmed the early native timer-count assertion prevents an unrelated missing-fixture panic. The reviewer ran no cargo, oracle or mutations.

This proves the sampled caller bodies under explicit imported contracts. General getters/proxies, Array species/reflection, arbitrary intrinsic replacement, full Promise microtask interleavings, concrete RPC/store/timer bindings and live websocket execution are unproved. The fixture supplies ordinary dense arrays/data objects and bounded intrinsic conversions; it is not a complete JavaScript engine or whole-class assembly proof.

Full workspace compilation, full venue suites, host/wasm clippy, wasm builds, canonical Rust-target checker, CI and live exchange/network tests were not run. The earlier canonical wildcard-import finding is recorded in `BUILDER_APPROVAL_READS_GATE_EVIDENCE.md`; no canonical pass is claimed here.

## Discriminating compiled controls

`/opt/homebrew/bin/python3.11 sdk-rs/safe-cex-venue-lighter/scripts/private-feed-helpers-mutations.py` exited **0**, anchored to frozen SHA `7aa2378520195d3da56de60f3fcd0ed046b0f3a5`. Every mutation compiled, failed at its named native assertion with **exit 101**, then passed its exact single test after byte restoration with **exit 0, 1 passed**. Enumeration stopped at five controls; no survivors in this floor. Compilation errors and wire/trace-only failures cannot satisfy the driver.

| Family / mutation | Exact test | Named native assertion | Broken / restored |
| --- | --- | --- | --- |
| Normalization: omit trim/uppercase | `private_feed_normalization` | `feed::native result status-primary` | 101 / 0 |
| Closed positions: use unrealized PnL for fallback | `private_feed_normalization` | `feed::native result closed-short` | 101 / 0 |
| Scheduler/throttles: set fired timer field to 1 | `private_feed_throttles` | `feed::native timer cleared throttleFetchOrders-default` | 101 / 0 |
| Subscription: swap auth yes/no log labels | `private_feed_subscribe` | `feed::native calls send-auth` | 101 / 0 |
| Live callee: bypass scheduler override | `private_feed_throttles` | `feed::native scheduled timer count throttleFetchOrders-override` | 101 / 0 |

Each invocation is `CARGO test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_private_feed_helpers -- <test> --exact`, with the TypeScript environment above. The driver restores source in `finally` and checks byte identity after all controls. A separate `git diff --exit-code HEAD -- sdk-rs/safe-cex-venue-lighter/src/ws_private_feed_helpers.rs` exited 0 afterward. Ignored logs/receipts live in `sdk-rs/target/lighter-private-feed-mutations/`; generated output is not committed. This sheet is manually authored evidence.

## Merge-down measurements

Fetched and merged integration `c3ef79745efde10bdd2b5f73b122f7e97051d9c4` down into this branch in merge SHA `645e03a88a4c2d92274bc493f9294ab51e0daf03`. The only conflict was `src/lib.rs`; resolution keeps both `ws_private_feed_helpers` and incoming `exchange_trading_preflight`. No CLAUDE conflict occurred.

The required whole venue source/test comparison, `git diff --exit-code 7aa2378520195d3da56de60f3fcd0ed046b0f3a5 645e03a88a4c2d92274bc493f9294ab51e0daf03 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests`, exited **1**: incoming preflight module/test and module exports changed. Whole-crate byte preservation is not claimed; affected gates were rerun.

| Measured merge SHA | Repeated command | Observed result |
| --- | --- | --- |
| `645e03a88a4c2d92274bc493f9294ab51e0daf03` | Same scoped `CARGO check` above | exit 0; Finished dev profile |
| `645e03a88a4c2d92274bc493f9294ab51e0daf03` | Same scoped `live_private_feed_helpers` target above | exit 0; 4 passed, 0 failed, 0 ignored |
| `645e03a88a4c2d92274bc493f9294ab51e0daf03` | `YARN parity:extract:check` | exit 0; Would update 0 manifests, 0 added, 0 orphaned symbols |

A separate named-input `git diff --exit-code` between those SHAs exited **0**, covering `src/ws_private_feed_helpers.rs`, `tests/live_private_feed_helpers.rs`, both `private-feed-helpers` scripts, owned private-feed manifest, existing transaction promise/signing/builders, all tracked `safe-cex-num` files and all three loaded source paths: `lighter.ws-private.ts`, `packages/safe-cex/src/types.ts`, `packages/logging/src/LogSeverity.ts`. Enumeration stops at that named set. The five mutations remain measured at their frozen SHA and were not repeated after merge.

Independent post-merge `tomllib` and real-target audit against integration `c3ef79745efde10bdd2b5f73b122f7e97051d9c4` exited **0**: exactly **8 pending → ported, 0 → divergent**, owned triple **(109, 6, 40)**, total 155. Full Lighter venue is **(245, 19, 72)**, total 336; other-lane gains are not attributed to this slice. Diff against that integration head lists only the owned manifest and six venue paths listed above, including this evidence sheet and module export.

No full suites, workspace build, canonical checker, host/wasm clippy, wasm build, CI or live network test was run after merge. No PR was created and no merge up into integration was performed.

Delivery edits after the measured merge are confined to this sheet; no implementation, test, script or manifest change is included in the evidence commit. Gate results above remain attached to their actual measured SHAs.

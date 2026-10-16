# Backpack private authentication and socket lifecycle

Code measured: `b37d1c365ebc3a7fc5c0e89dd2be12e90678e65d`.
Fresh branch: `feat/rust-backpack-private-auth`.
Actual parent: `972c5ec9bd8c4bb2023530199c45961dfca86229` (user-supplied integration head); no prior unreviewed Backpack branch is an ancestor outside integration.

Python 3.14 `tomllib` census over Backpack manifests compared each symbol with that parent: **10 pending → ported, 0 pending → divergent**. Parent: 42 ported / 53 divergent / 52 pending; code: **52 ported / 53 divergent / 42 pending**, 147 rows. Census is independent of proof.

Rows: public/private `connectAndSubscribe`, public `getDepthTickerUpdateThrottle`, private `onOpen`, `authenticate`, `subscribeToOrderUpdates`, `subscribeToPositionUpdates`, `dispose`, and exchange `listenOHLCV` / `listenOrderBook`.

The Rust bodies use compatible supplied property/call operations, preserving native UTF-16 values, error and callback identities, eager work before returning futures, asynchronous resumption, live subscription/signature mutation, and assignment/listener receiver evaluation order. Full socket adapters, cryptographic signing, and arbitrary JavaScript host prototypes remain outside this slice's proof; their pending rows were retained. Auth commands assert native method, params, signature values and identities before deferred wire comparisons; no decode/re-encode equality is used as native evidence.

## Measured exits at the code SHA

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2`: **0**.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test private_auth --jobs 2`: **0**, 10 passed / 0 failed.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**, output explicitly reports 7341 symbols / 512 files, would update 0 manifests / 0 added / 0 orphaned.
- `git diff --check` after all controls restored: **0**; tracked diff empty.

The required manifest writer was run before the code commit: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`, exit **0**, output updated 0 / added 0 / orphaned 0. No Rust-file arrays were hand-spelled.

Not run: workspace compilation, host clippy, wasm compilation/clippy, full crate or monorepo suites, live exchange/network tests, CI or CI reruns.

## Independent corpus floor

The actual-source Node oracle executes the three TypeScript Backpack classes using scoped dependency operations. All cardinalities and case inputs are independently pinned in Rust. **373 finite cases**, not an exhaustive JavaScript host enumeration:

| Section | Independent expectation | Last completed zero-based iteration |
| --- | --- | --- |
| Authentication | 196 = 28 authenticate + 140 order + 28 position; five optional UTF-16 symbols × existing membership × fourteen failure/resumption modes | Authenticate 27; order 139; position 27 (global 195) |
| Connections | 88 = public/private × disposed × testnet × eleven failure stages | Public 43; private 43 (global 87) |
| Private open | 16 = disposed × eight stages, including pending authentication and unawaited resubscribe promise | 15 |
| Private disposal | Eleven timer/socket access, throw, replacement and absence modes | 10 |
| Throttle cache | 50 = ten inputs × five failure stages; inherited properties, falsy numbers, lone surrogate, setter replacement, updater/factory map replacement | 49 |
| Exchange delegation | 12 = two methods × three UTF-16 inputs × success/error | OHLCV 5; order book 5 (global 11) |

## Compiling discrimination controls

All controls were applied to the measured code tree, run serially, restored byte-identically using the saved source bytes, then rerun with the same named test. Each command used the oracle environment and `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test private_auth --jobs 2 <test> -- --exact`. **Every mutation exit was 101 from a native/input assertion; every restored exit was 0, one passed / zero failed.** No compile-error control or surviving mutation is counted.

| Control iteration | Test | Mutation | Observed assertion |
| --- | --- | --- | --- |
| 0 | public_connection | Delete message listener registration | CONNECT native operation order 0 |
| 1 | private_connection | Defer connection execution until future polling | CONNECT_PRIVATE native eager operations 0 |
| 2 | private_open | Ping before resubscribe | OPEN_PRIVATE native callback order 0 |
| 3 | private_authenticate | Exchange native order/position stream order | AUTH native params |
| 4 | private_order_subscriptions | Substitute position stream prefix | AUTH native guard stream |
| 5 | private_position_subscriptions | Substitute order stream | AUTH native guard stream |
| 6 | private_disposal | Omit timeout unset | DISPOSE_PRIVATE native operation order 0 |
| 7 | depth_throttle_cache | Capture target map after updater/factory | THROTTLE native captured assignment target; updaterMap success case, cache iteration 40 |
| 8 | exchange_listen_ohlcv | Return a default native handle after forwarding | LISTEN_OHLCV native return identity |
| 9 | exchange_listen_order_book | Return a default native handle after forwarding | LISTEN_BOOK native return identity |
| 10 | private_authenticate | Delete one auth record after construction | AUTH independent corpus size, expected 196 / actual 195 |
| 11 | private_authenticate | Fill all 196 auth records with the first record | `auth_cases` input-mode assertion at tests/private_auth.rs:433, iteration 1: success versus independently expected pending |

Stopped after control iteration **11**. This is a floor over these twelve controls; no claim about untried mutations.

Restored SHA-256 pins: socket_lifecycle.rs `381cf2c686d535c1c396056f37033de2c27a191dafe634bb77d0e15c160986b0`; private_auth.rs `66fa8f161a7e6f75fc2d1b1b5fa4898bd036f2eb7fef5a2856115ee7affb7abb`; account_reads.rs `7079c0b2122620e707dda808871b39096f70f699b1a146d4751bd7fa251f8281`; private_auth_oracle.cjs `0f5388673ef5b01bdfa3aac94038e7666858743dfb9cc446dc41bc08220f84a5`.

## Review and development findings

Read-only sub-agent review found a real assignment-order gap: TypeScript captures the throttle map on the left-hand side before reading the updater and invoking the factory. The corrected Rust body captures that target first, writes it, then reads the current map. Independently reproduced with actual-source Node witness (reviewer reported exit 0), corrected before the code SHA, closed by follow-up sub-agent review. New source/native updaterMap and factoryMap cases plus control 7 cover it. No other concrete finding was reported within the documented operation scopes. The reviewer ran no Cargo commands.

An earlier uncommitted development run exited 101 with all ten tests failing because raw trace labels embedded a lone surrogate that serde_json could not decode. Only trace labels changed to numeric stream indices; UTF-16 argument/content assertions stayed intact. Restored uncommitted target passed, then the committed target above passed. This transport failure is not a mutation proof or a gate at the code SHA.

## Integration merge verification

Merged integration `602630e279b6a3eb284950215db6197a7d750eec` into this branch, producing measured merge SHA **`b36f4873068bc385623247cf69ecc87cfa492f44`**. `git diff --name-only` from the code SHA reports **zero** changed paths under this crate's `src/` and `tests/`, the TypeScript Backpack directory, and shared TypeScript types. Census at the merge SHA still reports 10 rows moved, all ported, 52 / 53 / 42.

Integration changed Cargo.lock (two other-venue dependency additions) and the transitive safe-cex-transport dependency; the original code-SHA gates are not presented as merge-SHA measurements. At this merge SHA, separately measured:

- The same scoped Backpack `cargo check --all-targets --jobs 2`: **0**.
- The same `private_auth` oracle test target/environment: **0**, 10 passed / 0 failed.
- The same extractor check: **0**; output would update 0 manifests / 0 added / 0 orphaned, 7341 symbols / 512 files.
- `git diff --check`: **0**.

Mutation proofs remain measured at the code SHA, not rerun or relabeled as merge-SHA proofs. The finite enumeration and unrun-gate limits above remain unchanged.

Pre-push merge-down fetched integration **`079b57e48d05cb74a0139613eb57f6253c3364ed`** and produced **`266d4c2ee78b9a196b7ec5f56d0348eecaeb4a9e`**. Compared with measured merge SHA b36f4873068bc385623247cf69ecc87cfa492f44, changed-path counts are zero for Backpack source/tests/Cargo.toml, its TypeScript oracle inputs, workspace Cargo.toml/Cargo.lock, and safe-cex-num/types/crypto/exchange/transport directories. Only the Coinbase exchange manifest changed in the manifest tree. Accordingly Cargo gates were not rerun or relabeled at this later SHA. A fresh extractor check at 266d4c2ee78b9a196b7ec5f56d0348eecaeb4a9e exited **0**, output would update 0 / add 0 / orphan 0, 7341 symbols / 512 files. `git diff --check` exited **0**; independent tomllib census remained **10 rows moved, all ported; 52 ported / 53 divergent / 42 pending**. The following evidence-only commit changes no measured source, tests, dependencies or manifests.

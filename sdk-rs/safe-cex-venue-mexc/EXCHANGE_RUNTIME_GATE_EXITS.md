# MEXC exchange runtime evidence

Implementation SHA: `79ea2047aa0c500b3eb9a2563895e137c47b86ef`.
Branch: `feat/rust-mexc-exchange-runtime`.
Actual starting parent: `74fc4846d408b2b29e843728c4233fe900abb371`, the integration head requested after the private-session collision correction. The earlier unpushed private-session branch is not an ancestor and contributes no row claim to this slice.

Independent Python 3.14 tomllib comparison against that parent: **8 pending → ported, 0 → divergent**. MEXC changes from 118 ported / 40 divergent / 27 pending to **126 ported / 40 divergent / 19 pending**, across 185 rows. This is a bounded eight-method-body slice, not complete exchange composition.

The new module ports tick, fetchBalanceAndPositions, cancelSymbolOrders, cancelAllOrders, setLeverage, changePositionMode, changeMarginType and dispose. It reuses the existing SessionRuntime/TaskHost primitives and their UTF-16 contract. Existing private-session start/observe bodies only gain pub(crate) visibility; their logic is unchanged. No shared type or shared trait was changed.

## Source proof scope

The oracle extracts the eight actual TypeScript AST members, retaining their bodies without rewrites. Source enums/constants and imported symbol/omitUndefined helpers come from their actual source. Existing child methods and I/O are bound at their call sites. Native values, object/error identities, cache contents and ordered effects are checked directly. No encoding/roundtrip comparison is a gate.

| Family | Independent floor | Last case reached |
| --- | ---: | --- |
| fetchBalanceAndPositions | 7 | positions-first-reject |
| tick | 14 | callback-live |
| cancelSymbolOrders | 8 | empty-orders |
| cancelAllOrders | 5 | empty-orders |
| setLeverage | 12 | update-config-error |
| changePositionMode | 10 | iterable-entry |
| changeMarginType | 12 | update-config-error |
| dispose | 7 | replace-map |

Total floor: 75 cases. No exhaustive-input claim. Pair probes assert both operations start eagerly, source result order and original balance/position references, reverse completion and rejection while the other operation remains pending. Configuration probes retain old objects and require fresh next objects plus shared stored-config identity. Map/timer probes cover live insertion, iterator pair close and replacing the timer map. Tick's retained callback reads the replacement method installed after scheduling. NaN is asserted by native classification; raw NaN payload-bit equivalence is not claimed.

## Earlier measured attempts

At `f2d8f92b89f29dc93c144b47d9f58b4ac45efffd`, the scoped exchange_runtime test exited 101 before assertions because a fixture field was unused. At `10562ce33a09a3a44a72272ed0c17d0bbcab77b3`, it exited 101 with 7 passed / 1 failed: `fetchBalanceAndPositions/balance-error eager effects native array shape`. The source fixture had thrown synchronously instead of returning its intended rejected promise. It was corrected without changing production behavior. At `39bc45c11dd14983f4a363cc85676dd3f1652f64`, all eight families passed (exit 0). Review then tightened no-match, callback and fresh-config probes; all eight passed again at the implementation SHA (exit 0).

The manifest writer ran before the implementation commit using `NODE_PATH=$PWD/sdk-rs/target/grvt-extract-deps/node_modules node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`: exit 0 and **Updated 1 manifests; 0 added, 0 orphaned symbols**. The resulting manifest was committed at the implementation SHA. Yarn output text was inspected.

## Gates measured at the implementation SHA

| Command | Exit / measured result |
| --- | --- |
| `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --all-targets --jobs 2` | 0; finished dev profile, zero errors |
| Same cargo executable, `test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --test exchange_runtime --jobs 2`, Node 24.20.0 | 0; 8 passed / 0 failed |
| Same scoped target with `test --offline` and `PATH=/opt/homebrew/opt/node@26/bin:$PATH`, Node 26.7.0 | 0; 8 passed / 0 failed |
| `NODE_PATH=$PWD/sdk-rs/target/grvt-extract-deps/node_modules node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; **Would update 0 manifests; 0 added, 0 orphaned symbols** |
| `git diff --exit-code 79ea2047aa0c500b3eb9a2563895e137c47b86ef -- sdk-rs/safe-cex-venue-mexc/src sdk-rs/safe-cex-venue-mexc/tests` after controls | 0; exact restoration |
| `git diff --check` | 0 |

The tests launch the source oracle with `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. All cargo calls were sequential. `--offline` was used for the final batched controls and Node 26 run with already-cached dependencies; it does not change the locked compiled input graph.

## Discrimination controls at the implementation SHA

Seven controls changed one production method each, ran its named test with `-- --exact`, restored exact bytes in `finally`, then reran that same test. All seven mutants compiled and reached the assertion below; all exited **101 broken → 0 restored**.

| Control / named test | Deliberate change | Native assertion reached |
| --- | --- | --- |
| balance_pair_native_source | Swap returned balance and positions | `fetchBalanceAndPositions/live` in assert_native's object-kind assertion |
| tick_native_source | Mark orders loaded in place of positions | `tick/live native object shape` |
| cancel_symbol_native_source | Invert strict requested-symbol filter | `cancelSymbolOrders/market ordered effects native string` |
| cancel_all_native_source | Update status to open instead of canceled | `cancelAllOrders/live ordered effects native string` |
| leverage_native_source | Define sellLeverage in place of buyLeverage | `setLeverage/live native number` |
| position_mode_native_source | Invert hedged selection | `changePositionMode/hedged native string` |
| margin_type_native_source | Set includeOpenTypeWithPositionId false | `changeMarginType/isolated eager effects native bool` |

The first disposal control exited 101 **before assertions**, because replacing the handle made its closure parameter unused under -D warnings. Its restored test exited 0. This is an instrument failure, not a valid control or a survivor. The corrected control consumes the original parameter and forwards 999; it was batched with three independent focused controls below. Each affects a different root method; the test hosts do not call another mutated root body.

One `cargo test --offline --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --test exchange_runtime --jobs 2` compiled all four mutants: exit **101**, 4 passed / 4 failed, with exactly the four named failures below. Restoring all four exact method bodies and running that same command exited **0**, 8 passed / 0 failed. No compiled mutant survived.

| Control / named test | Deliberate change | Native assertion reached |
| --- | --- | --- |
| dispose_native_source | Forward 999 instead of the original live timer | `dispose/live eager effects native number` |
| tick_native_source | Capture tick at scheduling instead of callback invocation | `tick/callback-live ordered effects native string` |
| position_mode_native_source | Read entry indices instead of iterator destructuring | `changePositionMode/iterable-entry native map floor` |
| leverage_native_source | Reuse the current config instead of spreading a fresh object | `native fresh next config identity` |

This completes one discriminating control for each of eight new row families, plus three focused controls for review findings. The initial noncompiled disposal attempt is excluded from that count.

## Reviews and unmeasured claims

Read-only production review by `/root/review_order_helpers` and fixture/manifest review by `/root/review_snapshots` approved `79ea2047aa0c500b3eb9a2563895e137c47b86ef` within the required-owner-binding method-body scope. Iterator destructuring, synchronous versus rejected leaves, no-match data, live callback lookup and retained/fresh config assertions were corrected. The reviewers ran no gates.

Not run: workspace compilation, full crate/monorepo suites, host clippy, wasm check/clippy, CI reruns, live-exchange/network tests or complete concrete exchange/socket adapter integration. Existing MEXC test suites are not covered by the new scoped target. Host and wasm remain separate claims.

## Merge-down preservation

Merged integration `7e9e5c660472d985e231394357b25ed171b929f2` into this fresh branch, producing `aafdcdc0d3f05a0059286068fcd811ca91123749`. Three independent `git diff --exit-code 79ea2047aa0c500b3eb9a2563895e137c47b86ef HEAD -- <scope>` calls each exited 0 for (1) MEXC src/tests, (2) MEXC manifests, and (3) shared Rust Cargo.toml/types/num/store/crypto/transport/exchange/candle-cache, cargo configuration, plus TypeScript MEXC/types/logging severity/utils and root package/lock inputs. The measured method bodies, tests and native/source input scopes are unchanged.

Cargo.lock changed only the unrelated safe-cex-venue-bitget package's dependency list. An independent tomllib dependency traversal from safe-cex-venue-mexc compared 481 reachable/conservatively selected locked package entries; every entry was byte-data-equivalent, and traversal stopped at closure exhaustion. Its script exited 0. The complete list of differing lock entries contained only safe-cex-venue-bitget. Cargo gates were retained on that unchanged MEXC input graph rather than rerun after merge.

The extractor was rerun at merge SHA `aafdcdc0d3f05a0059286068fcd811ca91123749`: exit 0, **Would update 0 manifests; 0 added, 0 orphaned symbols**. Independent committed-manifest tomllib census at that SHA against the actual starting parent exited 0: 8 pending→ported / 0→divergent, 126 ported / 40 divergent / 19 pending. `git merge-base --is-ancestor 9553f717809e25f94e25148c58b5db194c8d5577 HEAD` exited 1, proving the retired unpushed private-session implementation is not an ancestor. No unreviewed feature branch was used as the starting parent. The subsequent reporting commit edits this evidence file only.

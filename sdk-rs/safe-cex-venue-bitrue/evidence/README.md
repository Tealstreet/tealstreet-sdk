Latest merge-down evidence: [integration55d8addaf7](merge-55d8addaf7.md). The original measurements below remain bound to32b0651c16.

# Bitrue fallback balance retention repair

Measured code SHA: `32b0651c16ea5d586a4a77dfbf21ce2c84ae64bc`.
The later evidence-only commit does not change source, tests, manifests, or gate configuration.

## Ancestry and delivery

The inherited, unmerged PR 7380 head is `0514b6cf59a45628ffd8d61b1dcb620da48c1ce8`.
Merge `4e14730a72d24d60256e2455bd95f896e99c9b3c` has first parent that PR head and second parent integration `51156f6fbdd0ba338683dc61f3b92bad7441cb31`.
The merge was clean. This branch retains the inherited PR work as an ancestor; it is not based solely on reviewed integration code.
Work was performed on local `fix/bitrue-balance-borrow-guard` in the Crypto.com worktree, for fast-forward delivery to existing remote `feat/bitrue-rest-reads-next`; the parked Bitrue checkout was not modified.

## Behavior and discriminator

`Store::balance_handle` retains the current native balance allocation without retaining the central RefCell borrow. Both Store balance getters use that allocation. Balance update/reset replaces it; prior fallback results keep the previous object alive. Unrelated ticker writes preserve its identity. The owned Store snapshot/notification balance projection is synchronized before notification. The handle is obtained after the account await.

Regression `fallback_retains_balance_across_ticker_and_balance_writes` is an ordinary test, without `should_panic` or panic catching. It reads two fallbacks, writes an unrelated ticker while both results remain alive, replaces the balance, and resets the Store. Independent native expectations cover ticker count/value, old/new balance fields, repeated-read identity, replacement identity, and retained values after reset. There is no re-encoding comparison or wire assertion.

Before repair, at `1226b957690759e57d8ebfa745766b9bec3dbbf9`, the test compiled and **exited 101 at runtime**, panicking with `RefCell already borrowed` at `DefaultStore::mutate` on the unrelated ticker write. See `retention-before-1226b95769.log`.
At final measured code SHA it **exited 0**; see `retention-after-32b0651c16.log`. `git diff --exit-code 1226b95769 32b0651c16 -- sdk-rs/safe-cex-venue-bitrue/tests/balance_retention.rs` exited 0: the discriminator was byte-unchanged. This control reached the unrelated ticker-write iteration on the old code and completed through the reset iteration on fixed code; it is not an enumeration of every possible Store write.

The first repair SHA `b8811512b462d1a4b27d115a841291e935576d3a` passed native regression, four-package check, Bitrue/Store suites, and fmt, but strict host clippy exited 101 on an inherited Exchange `type_complexity` error. Wasm clippy and remaining gates were not run at that SHA. Its partial results are retained in `gates-b8811512b4.json`, not relabeled as final measurements. The follow-up commit only introduces a private transparent callback type alias in `position_intervals.rs`; the original callback type was byte-identical to integration `51156f6fbd`.

## Row movement

`census-32b0651c16.json` independently parses TOML with Python 3.11 tomllib, comparing integration, inherited PR head, and measured code SHA. This repair moves **0 additional statuses**. The inherited PR contributes **2 pending -> divergent** rows relative to integration: `fetchBalanceAndPositionsRaw` and `fetchBalance`. Final Bitrue counts: **15 ported / 25 divergent / 67 pending**. The census stopped after 6 Bitrue manifests, 107 rows, at `BitruePublicWebsocket.listenOrderBook` in `bitrue.ws-public.ts.toml`; it makes no repository-wide census or parity-proof claim. No divergence exemption was added; retained fallback behavior is recorded as CORRECTED within the existing divergent fetchBalance row.

## Review and limits

Native sub-agent static review at the measured code SHA found no blockers in the Store/Bitrue repair or transparent alias. The reviewer ran no Cargo gates.
The required new Store trait method is mapped to the existing Store manifest row. Production DefaultStore implements retained identity. Hyperliquid's read-observer fixture delegates; Exchange's existing fail-closed TestStore fixture explicitly panics if this unexercised method is invoked. No production fallback uses that fixture.
`Store::balance()` itself still returns a borrow guard; this repair changes the returned Bitrue fallback result to an Arc. It does not claim unrestricted JavaScript object mutation semantics or remove other existing input/numeric divergences.

Not run: workspace compilation or workspace suites, Exchange/Hyperliquid runtime suites, their standalone clippy gates, wasm runtime tests, ignored Store live-oracle tests, old Bitrue mutation scripts, live exchange requests, and CI reruns. Wasm clippy is a compile/lint claim, not a runtime claim. The exact-SHA table below records completed measurements.

## Completed gates at measured code SHA

| Gate | Measured exit | Scope |
| --- | ---: | --- |
| host-clippy | 0 | Store + Bitrue, all targets, strict warnings |
| wasm-clippy | 0 | Store + Bitrue, wasm32-unknown-unknown, all targets, strict warnings |
| scoped-check | 0 | Store + Bitrue + Exchange + Hyperliquid, all targets |
| retention-after | 0 | Identical Bitrue retention regression |
| bitrue-tests | 0 | Bitrue crate suite with maintained TypeScript oracle env |
| store-tests | 0 | Store crate suite; ignored tests remain ignored |
| fmt | 0 | Store + Bitrue + Exchange |
| canonical-index | 1 | Read-only repository index, building only safe-cex-parity; 34 inherited-file diagnostics |
| extractor-writer | 0 | Output: Updated 0 manifests; 0 added, 0 orphaned |
| extractor-check | 0 | Output: Would update 0 manifests; 0 added, 0 orphaned |

All Cargo commands ran sequentially; compiling commands used jobs 2. Exact commands, exits, times, SHAs, and log paths are in `gates-32b0651c16.json`. Extractor output was read rather than relying solely on the shim status. Tracked tree remained clean and the same SHA throughout these runs.

Observed bitrue-tests output: 12 passed, 0 failed, 0 ignored, across 14 test-result blocks. These are measured suite outcomes, not an exhaustive parity census.

Observed store-tests output: 54 passed, 0 failed, 4 ignored, across 12 test-result blocks. These are measured suite outcomes, not an exhaustive parity census.

The optional canonical index validation exited **1**, not 0. `canonical-findings-32b0651c16.json` preserves every diagnostic and verifies that all 22 compared files (finding-associated sources/manifests and validator/exemptions) are byte-unchanged from integration511 (diff exits0). No Bitrue or Store diagnostic appeared. The scan stopped at its 34th reported diagnostic, `safe-cex-venue-lighter/src/sdk_order_api.rs::ApiOrderBookOrderLevel`. The validator was not run at integration511; file comparisons do not establish a measured integration gate exit. No exemption or gate was weakened.

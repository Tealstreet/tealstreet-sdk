# WEEX account REST read evidence

This slice moves **8 pending rows to ported and 1 to divergent**: leverage maximum/selection, balance/positions composition, symbol configs, open-order rows, normal/algo order wrappers and combined orders. The divergent `fetchOrders` row is CHOSEN: rejected branches are reported through the host error sink while successful orders remain available; TypeScript silently drops those rejections.

Authoring parent: `983e9b13f32231bad4eb5c2cf76b6cd0c2a8adde`, the supplied merged integration head. No private unreviewed branch was used as an authoring ancestor. Implementation commit: `e7e4362a00ac1a6aa59a3f43e2e3841a783d4a96`. Exact-target manifest-note correction: `c2a8388fb5170f2b45243e5acc0ee22cd47a2e43`. Corrected fixture/control gate SHA: **`7b87b8a908e4026a95fc5559ae06166620648206`**.

## Gates measured at the corrected gate SHA

All commands ran serially with one cargo process at a time. The native test used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit | Observation |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2` | 0 | Scoped crate and its targets compile; not a workspace compilation claim. |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test account_reads` | 0 | `native_account_reads_match_typescript`: 1 passed, 0 failed, 54 oracle fixtures. |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py` | 0 | Canonical invention/status gate: 156 rows, 149 public Rust items; 0 WEEX exemptions. Global exemptions were 754 distinct entries, unchanged by this slice. |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Output: 7341 symbols; `Would update 0 manifests; 0 added, 0 orphaned symbols.` |
| `python3.12 -u sdk-rs/safe-cex-venue-weex/tests/account-reads-mutations.py` | 0 | Eight compiled production mutations failed named native assertions and passed after byte restoration. |

After manifest edits, the extractor WRITER ran: first `Updated 1 manifests; 0 added, 0 orphaned symbols.`, then after the note correction `Updated 0 manifests; 0 added, 0 orphaned symbols.` These were writer operations before their respective commits; the checked gate is the command/SHA pair above. Yarn was judged by output text as well as its measured exit.

At `e7e4362a00ac1a6aa59a3f43e2e3841a783d4a96`, scoped cargo check and the 54-fixture native test both exited 0, but the scoped parity wrapper exited 1: rename notes did not name the exact Rust targets. The notes were corrected without production/test changes. Extractor check was not reached in that aborted gate sequence. The complete green gates above were measured again at the corrected SHA.

## Mutation controls and the survivor finding

Each selected mutation below was measured at `7b87b8a908e4026a95fc5559ae06166620648206`; each compiled, produced `assertion left == right failed` at the named assertion with cargo exit 101, then passed the same target with exit 0 after restoring the original source bytes.

| Control / call site | Named native assertion | Broken / restored exits |
| --- | --- | --- |
| `fetch_symbol_max_leverage`: fallback 1 to 2 | `native_result::max_default` | 101 / 0 |
| `fetch_leverage_for_symbol`: buy reads short leverage | `native_result::leverage_isolated` | 101 / 0 |
| `fetch_balance_and_positions`: swap returned fields | `native_result::balance_positions` | 101 / 0 |
| `fetch_symbol_configs`: short fallback 20 to 21 | `native_result::configs_rows` | 101 / 0 |
| `fetch_open_order_rows`: algo page 1 to 2 | `native_requests::rows_algo_page` | 101 / 0 |
| `fetch_normal_orders`: retain falsey normalized orders | `native_result::normal_orders` | 101 / 0 |
| `fetch_algo_orders`: flatten array holes as undefined | `native_result::algo_sparse_flatten` | 101 / 0 |
| `fetch_orders`: discard rejected-branch error reporting | `native_order_failures_visible::orders_normal_failure` | 101 / 0 |

**Historical survivor, retained as a finding:** at `c2a8388fb5170f2b45243e5acc0ee22cd47a2e43`, the short-default mutation at `fetch_symbol_configs`, `(rt.to_number)(host, &short, 20.0)?`, changed 20 to 21 and the native test exited **0**. The fixture helper's JavaScript default argument had replaced the intended `undefined` with `'9'`, so no tested row exercised that fallback. The script restored the source in `finally`; `git diff --exit-code` on the source exited 0 and a separately run restored native test exited 0. The first three controls at that SHA failed/restored 101/0; the remaining four were not attempted after the survivor stopped the run.

Commit `7b87b8a908e4026a95fc5559ae06166620648206` supplies an explicitly undefined field and adds the direct native binary64 assertion `native_undefined_short_default == 20.0`. The same previously surviving mutation now fails `native_result::configs_rows` with exit 101, restores and passes with exit 0. Enumeration stops at the eight selected controls; no survivors remain among those eight after correction. This does not claim exhaustive mutation coverage.

## Scope of the evidence

The oracle extracts and executes the nine actual TypeScript class fields plus actual `extractArray`, `toNumber`, `omitUndefined`, venue constants and shared enums. Lodash implementations supply `uniqBy` and `omitBy`. Imported balance/order/algo normalizers, position fetching, transport, Store/Map operations, JavaScript property/coercion/iterator primitives and Promise scheduling are explicit capabilities. The test supplies bounded implementations of those dependencies; it proves these exchange algorithms and their dependency protocols, not every imported primitive or class assembly. Utility normalizers and the exchange class retain their own manifest statuses.

Fixture enumeration stops at **54**, checked against a literal ordered ID list. Native decoded values, IEEE-754 bits, UTF-16 strings, request fields, cache identity/content, captured old-cache contents, warnings and failure reports are asserted before any effect-trace comparisons across the full fixture floor. Source receiver capture, synchronous factory failures, retained peer requests, first-observed rejection, reverse successful completion, live symbol iteration, sparse flattening, partial config updates and push failure propagation have witnesses. The push witness injects a threshold of 1 into the source realm's array prototype: it proves delegation/error propagation, not Node's actual argument ceiling. Successful unpaired-surrogate computed-property keys, arbitrary prototypes/species, custom iterators and all scheduling permutations were not enumerated.

Read-only sub-agent review found the market receiver, config setter receiver, push-call primitive and successful completion-order gaps; all were addressed before the corrected gate run. The reviewer ran no builds, tests, mutations, edits or commits and found no remaining concrete blocker within the declared capability boundary.

Tomllib census against the authoring parent: 156 WEEX rows; 80 ported / 29 divergent / 47 pending before, 88 / 30 / 38 after. The nine transitions have existing Rust files and real target symbols; the canonical scoped checker independently validated all 149 public Rust items against the manifests. No shared type or shared runtime trait was changed.

Not run: full WEEX test suite, workspace compilation, full monorepo suites, host clippy, wasm clippy, live exchange requests or CI. No CI rerun, PR creation or merge into integration was performed.

## Merge-down and integration-relative gain

Fetched and merged the exact integration commit **`15925fadedb9338798d13cdf6168286aaeff6f15`**. Merge commit: **`47e3aa53767effc8527d85dd0214f7bcbcef600f`**. The merge was clean; no conflict resolution or history rewrite was needed.

`git diff --exit-code 7b87b8a908e4026a95fc5559ae06166620648206 47e3aa53767effc8527d85dd0214f7bcbcef600f -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests` exited **0**. A separate preservation diff covering the crate, root Cargo manifest/lock, shared types/num/store, WEEX TypeScript, shared enums, omitUndefined, logging severity, extractor/checker implementations, root package.json and yarn.lock also exited **0**. Thus compile, native and mutation results remain bound to `7b87b8a908e4026a95fc5559ae06166620648206`; they were not remeasured at the merge SHA.

The merge changed global exemption entries, other venue manifests and the manifest specification. The affected gates were rerun at **`47e3aa53767effc8527d85dd0214f7bcbcef600f`**:

| Gate | Measured exit | Output |
| --- | --- | --- |
| Scoped canonical parity wrapper | 0 | 156 rows, 149 public Rust items; 755 global exemptions, 0 WEEX exemptions. |
| Extractor check via the exact node/yarn command above | 0 | 7341 symbols; `Would update 0 manifests; 0 added, 0 orphaned symbols.` |
| Tomllib integration-relative census and new-target declaration audit | 0 | Exactly **8 pending→ported, 1 pending→divergent**, 11 real public Rust declarations for those nine rows. |

The census compares against the actual merge parent `15925fadedb9338798d13cdf6168286aaeff6f15`, not the stale authoring parent. Integration remained 80 ported / 29 divergent / 47 pending for WEEX; this branch is 88 / 30 / 38. Enumeration of this venue census stops at all 156 rows in its six manifests; it is a census, not proof of complete runtime equivalence. A subsequent commit updates only this evidence sheet.

## Silent rejected order legs (wave-1 correction)

This supersedes the historical choice above to report rejected `fetchOrders`
legs. The maintained source silently drops rejected Promise.allSettled results.
The real constructed exchange regression in commit `d1d79a149d` compiles on its
master parent and fails after an algo HTTP rejection: the ordinary order survives,
but the error emitter incorrectly receives `Exchange Error: algo endpoint unavailable`.
Removing the reporting capability makes that same regression pass with zero events.

Measured fixed worktree content (before its implementation commit):

- `src/account_reads.rs`: `ce2ef50c1bf94df2a1b3c459b436050041bfede23fc383735b2c32ce3e261eda`
- `src/binding/reads.rs`: `78e5f1289d6f0ae214a896d23ebed5bc73abedd6e34b5c2f8038992c37384ac4`
- `tests/account_reads.rs`: `302e26b80c5953c086d7d0cf1b8dc6424cd5bbd462954a28acd3d1c264990ae6`
- `tests/exchange_binding.rs`: `49e5d4cfa215a55ccdb37b89de9b357f2eefcc53bc508192ca1b9a9201f76165`

The actual-source account-read corpus retains all 54 fixtures and passes. The
full constructed binding target passes all 35 tests. Strict native and wasm32
all-target Clippy pass; cancellation inventory passes 6 tests with its intentional
exporter ignored. Scoped canonical full-resolution parity passes (7459 rows,
244 public Rust items, zero venue exemptions).

All eight account-read mutation controls compiled, failed their named assertions
with exit 101, and passed after exact restoration. After the equivalent Clippy
flatten rewrite, the changed `order-source-order` control was measured again:
reversing normal/algo source order fails `native_result::orders_unique` (101),
then restores and passes (0). This replaces the obsolete native-only reporting
control; it does not claim exhaustive mutation coverage. The default runner still
executes all eight controls, with an optional named-control selection for review.

Remaining continuation ownership and private/public refresh reporting choices
remain declared in MANIFEST-SPEC; this correction only fixes fetchOrders reporting.

Post-master merge `c83d2d5151` (master parent `4d4c5ffaba`): all 38 binding
tests, the 54-fixture source corpus and cancellation inventory (6/1) pass. The
direct node extractor check reports zero manifest updates. Full-resolution parity
now fails on the unchanged shared BaseWebSocket.recoverAfterOffline declaration
pointing to absent JsProtocol::recovers_after_offline; this is a master baseline
failure outside the WEEX changes, reported to the overseer.

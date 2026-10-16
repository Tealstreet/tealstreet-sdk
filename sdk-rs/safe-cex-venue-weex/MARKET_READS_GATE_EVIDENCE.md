# WEEX market-data and execution read gates

This coherent exchange-file unit moves **9 pending rows to ported, 0 to divergent**: `fetchMarkets`, `fetchTickers`, `fetchOHLCV`, `listenOHLCV`, `listenOrderBook`, `fetchOrderBook`, `fetchPublicTrades`, `fetchHistoricalFills`, and private `fetchUserTradeRows`.

Authoring parent: **`6a15df74a80721009ec4b07ea1c4236c1d3242e7`**, the supplied merged integration head. No private unreviewed branch was used as an authoring ancestor. Implementation and measured gate SHA: **`9991325ff4fe9265b973ac1fb0e147f6b1817d67`**. Branch: `feat/rust-venue-weex-market-reads`.

## Measured gates at the implementation SHA

Cargo commands ran serially, one cargo process at a time. The native target used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit | Observation |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2` | 0 | This crate and its targets compile; not workspace compilation. |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test market_reads` | 0 | `native_market_reads_match_typescript`: 1 passed, 0 failed, 60 TypeScript oracle fixtures. |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py` | 0 | Canonical invention/status gate: 156 rows, 160 public Rust items; 776 global exemptions and 0 WEEX exemptions. This slice adds no exemptions. |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 7341 symbols; `Would update 0 manifests; 0 added, 0 orphaned symbols.` |
| `python3.12 -u sdk-rs/safe-cex-venue-weex/tests/market-reads-mutations.py` | 0 | All eight compiled controls failed named native assertions with 101 and passed after restoration with 0. |

The extractor WRITER ran after manifest edits before the implementation commit and printed `Updated 1 manifests; 0 added, 0 orphaned symbols.` That is a writer operation, not a gate measured at the later SHA. The checked gate is the exact command/SHA pair above; yarn was judged by its output text as well as measured exit.

## Compiled mutation discrimination

Each mutation below was measured against `9991325ff4fe9265b973ac1fb0e147f6b1817d67`. Every broken test compiled and failed the named decoded native assertion; each original source was restored byte-for-byte in `finally`, and the same target then passed with 0. The final source/test worktree diff against the implementation commit exited 0.

| Family / mutation | Named native assertion | Broken / restored exits |
| --- | --- | --- |
| Markets: inspect quote asset in place of margin asset | `native_result::markets_filter` | 101 / 0 |
| Tickers: swap ticker and book normalizer inputs | `native_result::tickers_join` | 101 / 0 |
| Candles: route historical requests to recent endpoint | `native_requests::candles_historical_zero` | 101 / 0 |
| Subscription routing: use order-book method for candle subscription | `native_result::listen_candle_provider` | 101 / 0 |
| Depth: read the wrong update-ID field | `native_result::depth_default` | 101 / 0 |
| Public trades: change lower limit clamp 1 to 2 | `native_requests::public_limit_lower` | 101 / 0 |
| Execution query: reverse the from-bound comparison | `native_result::query_filters` | 101 / 0 |
| Historical fills: sort ascending instead of descending | `native_result::history_order` | 101 / 0 |

Enumeration stops at these eight selected controls, covering all new method families. There were no survivors among them; this is not exhaustive mutation coverage. The query gate checks actual decoded rows in addition to an independent literal boundary count, so it can detect different rows with the same count.

## Evidence scope and limits

The oracle extracts the nine actual TypeScript class fields and actual `extractArray`, `toIntegerString`, `normalizeTimestampToMs`, `omitUndefined`, venue constants and shared enums. It uses the installed Lodash `omitBy`. Imported normalizers, provider/websocket methods, property/coercion operations, collection builtins and transport/Promise work are explicit host capabilities. The native functions implement the request, filtering, joining, ordering, fallback and routing algorithms over opaque caller values. Imported utility implementations and class assembly keep their own manifest statuses.

Enumeration stops at **60 fixtures**, asserted against an independent literal ordered ID list. Additional native floors assert 2 selected query rows and 3 retained candle rows. Decoded native errors/results, binary64 number bits, UTF-16 strings, endpoint/params/timeout fields, reports, bound receivers, arguments and callback identities are checked before any effect traces across the complete fixture floor. Expected values are decoded from the source result; native values are never re-encoded or roundtripped.

Witnesses cover USDT filtering, last duplicate ticker key, numeric/Symbol String keys, both GETs starting eagerly, first-observed dual rejection and peer survival, response `.data` reads after aggregation, undefined-based historical routing, repeated option getters, defaults/clamps including NaN and signed zero, stable timestamp ordering, holes, a throwing species constructor, provider's repeated read and receiver binding, normalization before last-update-ID reads, exact order-ID/time filters, public-only early return, live row-symbol reads with the market receiver captured between them, and visible venue-message/error-message fallback.

Read-only sub-agent review found two production gaps: String-constructor versus template Symbol coercion, and premature response-data projection. Both were corrected before the measured gates, with source/native witnesses. Follow-up review found no remaining concrete blocker within the declared capability boundary. The reviewer ran no builds, tests, mutations, edits or commits; the mutation script was not part of that review.

The test's collection/scheduler/coercion/normalizer implementations are bounded models. Successful custom species, arbitrary prototypes/custom iterators, observable sort-comparator coercion/getter permutations, all date-string/numeric conversions, wider scheduling permutations and the imported normalizers' full behavior were not enumerated. The throwing-species case proves error propagation through the supplied primitive, not a universal species implementation.

Tomllib census against the authoring parent: **156 WEEX rows**; before 88 ported / 30 divergent / 38 pending, after 97 / 30 / 29. Each transitioned row has a real Rust target, and the canonical scoped gate validates all 160 public Rust items. No shared SDK type or runtime trait was changed.

Not run: full WEEX test suite, workspace compilation, full monorepo suites, host clippy, wasm clippy, live venue requests or CI. No CI rerun, PR creation, integration merge-up or income pager work was performed.

## Pre-push merge-down

Fetched and merged the exact integration commit **`9916944cd54d8861db6b81f96878fa6082ff4739`**. Merge commit: **`84f718786f9f05c8b4952e275d79ddee554cfe5b`**. The merge was clean; no conflicts or history rewrite occurred.

`git diff --exit-code 9991325ff4fe9265b973ac1fb0e147f6b1817d67 84f718786f9f05c8b4952e275d79ddee554cfe5b -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests` exited **0**. Shared Rust source, WEEX TypeScript, helper/enums/logging source and extractor/checker implementations were also byte-unchanged. A broader gate-input preservation diff exited **1**, correctly identifying changes in `sdk-rs/Cargo.lock` and `package.json`; global parity inputs changed as well.

The four baseline gates were rerun and measured at **`84f718786f9f05c8b4952e275d79ddee554cfe5b`**:

| Exact command from the baseline table | Measured exit | Observation |
| --- | --- | --- |
| Scoped cargo check, all targets, jobs 2 | 0 | Scoped crate/targets compile. |
| Native `market_reads` test target, jobs 2 and the same TypeScript environment | 0 | 1 passed, 0 failed; the same 60-fixture floor. |
| Scoped canonical parity wrapper | 0 | 156 rows, 160 public Rust items; 839 global exemptions and 0 WEEX exemptions. |
| Extractor check via node/yarn | 0 | 7341 symbols; `Would update 0 manifests; 0 added, 0 orphaned symbols.` |
| Tomllib integration-relative census plus target declaration audit | 0 | **9 pending→ported, 0 pending→divergent**; 11 real public Rust declarations for those nine rows. |

The census uses the actual merged integration parent `9916944cd54d8861db6b81f96878fa6082ff4739`, rather than the authoring merge-base. Integration remained 88 ported / 30 divergent / 38 pending for WEEX; this branch is 97 / 30 / 29. This venue census stops at its 156 rows in six manifests; it is a census rather than complete runtime proof.

Mutation exits remain bound to **`9991325ff4fe9265b973ac1fb0e147f6b1817d67`** and were not remeasured at the merge SHA. The source/tests and primitive implementation inputs are unchanged. Parsing both Cargo.lock versions showed exactly one changed package record: `safe-cex-venue-okx` added dependencies; WEEX and all of its dependency records are unchanged. The root package edit only extends the Kucoin oracle script. A subsequent commit updates only this evidence sheet.

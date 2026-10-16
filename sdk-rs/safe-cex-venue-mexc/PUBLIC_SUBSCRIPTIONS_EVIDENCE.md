# MEXC public subscription body ports

Eight rows move **pending → ported**, zero move to divergent. Independent tomllib census: **102 / 40 / 43 → 110 / 40 / 35** (ported/divergent/pending), 185 rows in six manifests. [Exact changed-row census](evidence/public-subscriptions/census.json). Enumeration stops at `mexc.ws-public.ts.toml`, the sixth manifest. Census is separate from behavior proof and target resolution.

Branch: `feat/mexc-public-subscriptions-3a9577`. Actual starting parent: **3a957718fe445a833bb10a464e16e7cc6c585276**, directly on integration. No unreviewed lane branch was used as an ancestor. Integration was merged down before gates and again before push; never merged into integration and no PR opened.

Measured gate SHA: **0fa3759e194cf72ea3005ff44796aebb938cc063**. [Commands/exits](evidence/public-subscriptions/gates.json), with raw logs alongside. Cargo commands ran sequentially, locked, jobs2. Source oracle used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate at the measured SHA | Measured exit/result |
| --- | --- |
| Scoped MEXC cargo check, all targets | 0 |
| Scoped MEXC cargo test | 0; 130 passed, zero failed; doc tests ran zero cases |
| Scoped MEXC fmt check | 0 |
| Official MEXC-only invention/status checker | 0; 185 rows, 233 indexed public items, zero exemptions |
| Actual Yarn parity:extract:check | 0; inspected output: would update 0 manifests, zero added/orphaned symbols |
| Eight compiled controls | Driver 0; shared baseline0, each mutation101 and exact restored test0 |

The original source bodies are callable Rust body ports through native receiver/runtime boundaries. Class construction and production MexcExchange websocket composition remain existing **FINDING** rows. Legacy `MexcPublicProtocol` callbacks were not silently replaced or claimed equivalent. Candle normalization is a child seam; the original symbol utility is exercised only on its underscore path. Dense ordinary typed symbol arrays are the reconciliation projection contract; arbitrary JS proxy/species/accessor arrays or a general JS runtime are outside this fixture proof.

## Native assertions and discrimination

Original TypeScript arrow initializers are extracted by AST and evaluated with the fixture receiver as their lexical `this`; method-body text is not rewritten. The oracle uses the original URL/interval declarations, subscriptionKey, LogSeverity and underscore-path toMexcSymbol utility. Native assertions directly inspect argument values, ordered subscription fields, UTF-16 code units, Number bits, original callback/error/handler/subscription/data identities, live iterator observations, partial state after failures and deferred callback results. Subscription schemas/counts use fixed fixture roles, with method asserted before keys/counts. Native record shape is asserted before the fixture's stringify intrinsic. There is no native decode/re-encode, roundtrip, or native wire-byte comparison; strings observed at child/runtime call boundaries remain native argument values.

The named trace counts are independently fixed from source call-site expectations. For example cap fixture10 has `2 + 100×3 + 6 + 2 + 99×5 = 805` events; missing-first-market fixture11 has `2 + 1 + 99×3 + 9 + 99×5 = 804`. Final map, callback and cleanup counts use fixed scenario expectations. Input payloads and routed keys are independent constants, not taken from captured source output.

| Isolated compiled production fault | Named native assertion | Baseline / mutation / restored |
| --- | --- | --- |
| connectAndSubscribe: remove disposal guard, retain flag read | `native independent trace count connectAndSubscribe/1` | 0 / 101 / 0 |
| listenOHLCV: omit subscribe | `native independent trace count listenOHLCV/0` | 0 / 101 / 0 |
| handleActiveTickerSymbolsChanged: omit reconcile | `native independent trace count handleActiveTickerSymbolsChanged/0` | 0 / 101 / 0 |
| reconcileTickerSubscriptions: cap100 → cap99 | `native independent trace count reconcileTickerSubscriptions/10` | 0 / 101 / 0 |
| resubscribe: omit ticker-map pass | `native independent trace count resubscribe/0` | 0 / 101 / 0 |
| sendWithDiagnostics: omit send | `native independent trace count sendWithDiagnostics/0` | 0 / 101 / 0 |
| emitReceiveDiagnostics: global String → abstract ToString | `native formatter expected failure` at Symbol fixture | 0 / 101 / 0 |
| handleRoutedMessage: stop before appended third handler | `native independent trace count handleRoutedMessage/8` | 0 / 101 / 0 |

[Control summary](evidence/public-subscriptions/controls/summary.json) and 17 raw baseline/broken/restored logs retain all eight attempts. Each fault compiled and failed its named runtime assertion; no survivor or wrong failure occurred in this eight-control floor. The script restores exact source in finally and verifies the final bytes. Reviewers stopped reading before faults were applied.

Proof floors: **8 method families × 12 fixtures = 96 root calls**, stopped at fixture **11** per family and whole-corpus fixture **95**, handleRoutedMessage/11. Retained candle handler replay stops at its first invocation; successful cleanups run twice, stopping at invocation1. Diagnostics formatters replay ten values through index9; receive predicates replay three states through index2. Live iteration witnesses include map deletion/append, registry replacement, handler append and a hole; these are bounded witnesses, not exhaustive enumeration of possible mutations. Available-market inputs stop at 102 slots in cap fixture11; source cap restricts lookup to the first100 before the missing-market skip. Mutation enumeration stops after handleRoutedMessage, the eighth control.

## Earlier attempts and review

[Earlier SHA be1ab27c2680deba0a1de0aa5d1ffbec7c3bc55c](evidence/public-subscriptions/attempt-be1ab27c/gates.json): check0, tests0/130 passed, fmt0, target-checker1; extractor was **not run** in that attempt. Six real public record fields used unsupported `::field` index paths. They were removed from rs declarations, retaining the containing record types; no checker or exemption was changed. The final checker resolves all off-pending rows and indexes 233 items (prior156 plus77 new indexed items). The first manifest-update diagnostic's83 included the six non-indexed fields and is not the final indexed count. Both manifest edits used parseManifest/serializeManifest and the actual extractor writer; both writer runs exited0 with zero added/orphaned symbols. Raw outputs are retained.

During uncommitted development at the starting parent, a source-oracle draft exited1 on an independent param-key assertion: legacy kline fixture records lacked interval. The fixture was corrected; the corrected standalone oracle and first native draft both exited0. Those dirty runs are not gates covering a committed SHA; the first draft stderr was not retained as a raw file. Final committed gates supersede them.

[Static sub-agent review](evidence/public-subscriptions/review.md) covered source/bodies, native tests, oracle, control anchors/assertions and mappings. Artifact-derived subscription expectations were replaced with fixed fixture roles, and exact cleanup subscription Rc identity was added. Follow-up found no remaining actionable issue. Reviewer ran no Cargo/oracle/tests/controls.

## Final merge preservation

Last pre-push merge includes integration **91bdde133d2f407dffa8f2140f94e87eac7c1d5f** at comparison head **6f72b626d3b8dc1f2d81ef9b4dc8a3cc61778749**. [Receipt](evidence/public-subscriptions/merge-preservation.json): own src/tests diff exit0; extended29 paths (scripts, target manifests, actual TS trees, seven dependency source/config pairs, workspace Cargo.lock/config and checker inputs) diff exit0. MEXC-filtered exemptions remain independently expected0. Package.json raw diff exited1: only the unrelated Kucoin oracle script gained `--test rest_reads`; strict parsed equality after removing that one script passed. MEXC build/oracle/extractor inputs are unchanged. Previous scoped results stand at their measured SHA; no fresh scoped Cargo/control gate at the merge head is claimed.

A fresh [global extractor at the merge head](evidence/public-subscriptions/merge-extractor.json) exited0; [raw output](evidence/public-subscriptions/merge-extractor.log) says would update0 manifests, zero added/orphaned symbols. The subsequent evidence-only commit changes no gate inputs.

## Not run

No workspace compile/test, full monorepo suite, host clippy, wasm clippy, CI rerun, Polymarket gates, real network, autonomous socket lifecycle or general candle/symbol-normalization proof. The other35 MEXC pending rows remain pending; class construction/composition FINDINGs remain.

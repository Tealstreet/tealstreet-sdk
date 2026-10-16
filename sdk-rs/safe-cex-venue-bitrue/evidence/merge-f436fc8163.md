Superseded by [the urgent55 merge](merge-55d8addaf7.md). This historical sheet does not cover the latest SHA.

# PR 7380 integration merge-down

Measured code SHA: `28e0c6039e58418e8f953e7aff4cc2c579cdaa49`.
Parent `969b35013c` adapts the regression Ticker initializer with `timestamp: None`; parent merge `7fdc9e3e114117c3815e8b3265d7622c783ae643` has parents previous PR tip `3ea80be3b5b5a4f979583b98336ef3e7fbe5d4e7` and integration `f436fc816390a8b4556ab7a2fe0ae85072337036`.
The branch retains the unmerged PR 7380 work as ancestry. The only merge conflict was Store CLAUDE documentation: both sides were retained by removing conflict markers, without rereading the guidance.

Bitrue production source is byte-unchanged from prior measured `32b0651c16`; its regression gained only the integrated optional Ticker timestamp field, with all assertions unchanged. Integrated Store now copies that timestamp during ticker updates; Exchange has new owned-task/runtime code. The old gate exits are therefore not claimed for this merged code.

## Control and compile failure retained

At base SHA `969b35013cf775be5b8b134008f34843f917e881`, the negative control temporarily replaced only Bitrue account_reads.rs with the pre-fix `1226b95769` source. It compiled and exited **101 at runtime**, with the named `fallback_retains_balance_across_ticker_and_balance_writes` test panicking `RefCell already borrowed` on its unrelated ticker write. The exact mutation is `old-guard-control-969b35013c.patch.json`; its log and `gates-969b35013c.json` explicitly identify this dirty variant, rather than presenting it as clean code at that SHA. The runner restored production bytes in a finally block; the clean-tree regression then exited **0** at969.

The subsequent four-package all-target check at969 exited **101**: integration's Hyperliquid state-test Clock lacked the newly required ExchangeRuntime::monotonic_now_ms. That fixture file was byte-unchanged from integration f436. Commit28e0 adds a fail-closed panic for unexpected monotonic-timer use, consistent with the fixture's unsupported sleep/spawn paths. It does not alias wall time to monotonic time. Relevant constructor/helper paths use now_ms; targeted state tests verify the mock remains usable.

The Store, Exchange, Types, Bitrue source and regression files used by the negative control are byte-unchanged between969 and final measured28e0; comparison exits0 are in `merge-byte-comparisons-28e0c6039e.json`. The final clean regression was rerun at28e0. Its native assertions precede any wire assertions (there are no wire comparisons); independent literal expectations and old-result identities remain intact. The negative control stops on its ticker-write iteration, and the restored regression completes through reset; this is not exhaustive coverage of every Store write.

## Status movement and review

`merge-census-28e0c6039e.json` independently reads TOML using Python3.11 tomllib against the previous PR tip. **0 rows moved** during this merge-down. Bitrue remains **15 ported / 25 divergent / 67 pending**. Its census stops after6 manifests/107rows at `BitruePublicWebsocket.listenOrderBook`; it makes no other-venue census or comprehensive parity-proof claim. The existing PR's two pending->divergent rows remain inherited, not new movement.

Native sub-agent review of committed969 and28e0 found no blockers. The reviewer read no CLAUDE/memory and ran no Cargo. Production retention/identity paths are unchanged; the fixture timestamp addition preserves assertions; the clock adaptation fails visibly if state helpers unexpectedly use monotonic time.

Not rerun at28e0: canonical repository index (the earlier exit1 at32 remains an earlier finding, not a latest gate result), full workspace compilation/suites, Exchange runtime suite, full Hyperliquid suite or standalone host/wasm clippy, wasm runtime tests, ignored Store live-oracle tests, old mutation collections, exchange network requests, and CI. Positive compiling Cargo gates run sequentially with jobs2. The final evidence commit adds documentation/logs only and does not change measured source/tests/manifests.

Historical completed gates: see gates-28e0c6039e.json; its formatter exit1 is retained. Writer/check were not run at28e0 before the urgent newer merge.

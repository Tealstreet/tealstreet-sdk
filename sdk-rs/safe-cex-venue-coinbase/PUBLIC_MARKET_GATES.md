# Coinbase public market-data slice — measured gates

Reviewed integration parent: `aa29619aa6374dec932430ce9b2c474d527ea24e`.
Code/metadata gate SHA: `41b0d1361bb1004237fc901e7b0a63d416dca003`. Fresh branch
`feat/rust-coinbase-public-market-data`; no separate unreviewed lane was used
as its parent. The earlier implementation commit `459df499df` belongs to this
same new slice and is included in the gated SHA.

Tomllib census at that SHA, stopped after all 16 Coinbase manifests / 426
rows: **386 ported / 17 divergent / 23 pending → 394 / 17 / 15**.
**8 pending→ported, 0→divergent.** Every moved target resolves to its committed
Rust body; Data explicitly resolves to the active PublicData enum, not a
fabricated function. Scoped AST mapping independently passed.

Mapped rows: fetchMarkets, fetchMarketsFromIntxInstruments, fetchOHLCV,
exchange listenOHLCV/listenOrderBook delegates, Data, and public websocket
listenOHLCV/listenOrderBook. Full exchange/socket assembly stays pending.

| Gate actually run at the SHA above | Measured exit |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2` | 0 |
| Scoped Coinbase tests: lib, public_market_data, public_stream_dispatch, ticker_position_reads, portfolios_balances, decoder_boundaries | 0; 29 passed |
| Extractor check via direct Node/Volta Yarn entry point | 0; output `Would update 0 manifests; 0 added, 0 orphaned symbols.` |
| Unchanged scoped AST parity checker | 0; 426 rows / 796 public Rust items |

`PUBLIC_MARKET_GATE_RESULTS.json` retains exact commands and complete output.
Extractor output was inspected; no success claim comes from Yarn exit status
alone. The writer ran after the two manifest edits and reported two manifests
updated, zero additions/orphans. During development an invalid `confidence`
field and initially absent mapping layers were rejected and corrected; those
precommit attempts are not passed gates at this SHA.

The live source oracle extracts all seven field initializers plus constants
and imported normalizer/math/orderbook bodies. It uses actual dayjs with UTC
plugin and a fixed clock. The content-pinned corpus floor is **22 read cases
and 16 listener cases**, stopped at these 38 cases; focused source assertions
also cover forwarding, retained callback arrays, eager/fail-fast requests,
empty recovery arrays and rejection adoption. No unbounded-input proof is
claimed. Seven native tests cover these cases plus timing and retained roots.

Native Market/Candle/Data/book values, own undefined properties, flags,
f64 number bits and Rc identities are asserted directly before typed HTTP/
subscription boundary assertions. No decoded-native/re-encoded comparison is
used as the oracle. Counts come from literal inventory/corpus floors or the
independent TypeScript result, never the native artifact's own loop bound.
Typed listener call storage preserves original subscription/options/delay/
Frame values before assertion. Topic key sets and step counts are checked.

The scope is declared typed reply projections and inert acyclic JSON records
with retained roots and pure imported dayjs/math/orderbook contracts. Ordinary
daily-cache inherited-name lookup is an explicit host capability, including
constructor in the corpus. Exotic getters/proxies, cyclic object graphs and
full transport construction are outside this slice. Read-only sub-agent
review found no remaining blockers within this declared scope; the reviewer
ran no Cargo/gates and its review is not substituted for the measurements.

Compiled mutation floor: **nine controls, stopped after the rows below**.
Every altered production/fixture body was compiled; the named test failed
with exit 101, source bytes were restored, and the named test passed with
exit 0. No survivors occurred in this nine-control floor. This is not an
exhaustive mutation census. Full output, old/new expressions, call sites and
original SHA256 values are retained in `PUBLIC_MARKET_MUTATION_RESULTS.json`.

| Control | Named test | Failing native assertion | Mutated / restored exits |
| --- | --- | --- | --- |
| catalog-contract | `public_reads_assert_native_catalog_candles_and_query_bits_against_live_source` | assertion `left == right` failed: catalog spot intx cfm: native bits | 101 / 0 |
| intx-filter | `public_reads_assert_native_catalog_candles_and_query_bits_against_live_source` | public helper filtered: original native market info root | 101 / 0 |
| historical-reverse | `public_reads_assert_native_catalog_candles_and_query_bits_against_live_source` | assertion `left == right` failed: candles newest cache identity.timestamp: native bits | 101 / 0 |
| data-spread | `public_listeners_assert_native_records_book_levels_and_retained_roots_against_live_source` | assertion `left == right` failed: daily retained unknown fields own undefined close: generated own fields | 101 / 0 |
| candle-trade-volume | `public_listeners_assert_native_records_book_levels_and_retained_roots_against_live_source` | assertion `left == right` failed: five minute native root then trade spread: native bits | 101 / 0 |
| book-snapshot-side | `public_listeners_assert_native_records_book_levels_and_retained_roots_against_live_source` | assertion `left == right` failed: book update before snapshot snapshot delta reset: native side count | 101 / 0 |
| candle-delegate | `delegates_and_listener_registration_preserve_native_roots` | delegate original options root | 101 / 0 |
| book-delegate | `delegates_and_listener_registration_preserve_native_roots` | assertion `left == right` failed: delegate unnormalized input symbol | 101 / 0 |
| fixture-same-length-replacement | `public_reads_assert_native_catalog_candles_and_query_bits_against_live_source` | assertion `left == right` failed: public corpus content pin | 101 / 0 |


Not run: workspace/monorepo builds or suites, the entire Coinbase test suite,
host clippy, wasm clippy/compilation, CI reruns, or live venue/network tests.
The check is scoped crate/all-targets host compilation; it is not workspace
compilation or wasm verification. Merge-down and unchanged-source provenance
are recorded separately after fetching the actual integration head.


## Merge-down provenance

Fetched reviewed integration `039d308cd99cfd09e22205fb1083c3df5a306548` and merged it cleanly into
this feature branch at `febceca79c583b26c81fcf538c5a0387e2721564`. Compared against gated source SHA
`41b0d1361bb1004237fc901e7b0a63d416dca003` with scoped `git diff --exit-code`:
source/tests exit **0**, and script/oracle inputs/Coinbase manifests exit **0**.

The integration merge added crypto modules and Cargo.lock entries; no existing
imported behavior used by this slice changed. Re-ran scoped all-targets check,
extractor check and scoped AST mapping at the merged SHA: exits **0 / 0 / 0**;
extractor text again confirms zero updates/additions/orphans. Exact commands,
outputs and preservation exits are in `PUBLIC_MARKET_MERGE_RESULTS.json`.
The 29 selected tests and nine mutation controls were **not rerun after the
merge**: their measured exits remain explicitly bound to the original gate
SHA, with the unchanged code/test/oracle proof above. This is not a new test
measurement at the merged SHA.

Independent tomllib census against the fetched integration head at the merged
SHA confirms the true gain is still **8 pending→ported, 0→divergent**, and the
Coinbase triple is **394 ported / 17 divergent / 15 pending**. See
`PUBLIC_MARKET_FINAL_CENSUS.json`; this enumeration stops after 16 manifests /
426 Coinbase rows and makes no other venue claim. The final evidence commit
contains documentation/measurement files only.

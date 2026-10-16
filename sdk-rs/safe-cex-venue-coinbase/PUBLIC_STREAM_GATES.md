# Coinbase public websocket dispatch

Eight pending→ported rows: connectAndSubscribe, onOpen, subscribe, onMessage, handleOhlcvStreamEvents, handleTradeStreamEvents, handleOrderBookStreamEvents and ping. Authoring parent `654fd1aedc3de8fe6701088ec6dca71dfe897ace`, a reviewed integration head; no unrelated unreviewed feature ancestor.

Gate SHA `1e3de14942ccb7d0c2730cf18f0cca02a7e4f9df`. Commands, exits, native test output, controls and tomllib census are tracked in `PUBLIC_STREAM_GATE_RESULTS.json`.

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2`: **0**.
- Scoped `cargo test` with `--lib --test public_stream_dispatch --test decoder_boundaries --jobs 2`: **0**, 14 tests (9 library, 4 new native, 1 decoder-inventory). Oracle environment `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
- Extractor check: **0**, output reports zero manifests updated / zero added / zero orphaned. Writer ran twice after manifest changes; each output reports one updated manifest, zero added/orphaned.
- Coinbase scoped invention/status mapping: **0**, 426 rows /659 public Rust items. Twenty-seven actual native state/host/payload/callback helper targets have explicit exemptions; all eight moved rows have real committed function targets.

## Compiled discrimination

Three controls ran sequentially at the gate SHA. The book selector first matched routing and statistics and aborted before mutation (process exit1); selector fixed at `3e9a35206e`, where the book-only control ran. Source/tests diff versus the gate SHA exited0. Each control result records its actual SHA. Each compiled mutation failed `public_stream_live_native_corpus` **101** at the named native assertion, restored source and passed **0**. Enumeration stopped after control4; no survivors. Source/tests restore diff **0**.

| Family/control | Named native assertion | Broken/restored |
| --- | --- | --- |
| Lifecycle endpoint | connect-live: public endpoint |101/0|
| Candle open/close swap | candles-grouped-aliases.open: decoded native number bits |101/0|
| First trade replaced by last | trades-first-only-raw-symbol: native UTF16 string |101/0|
| Book raw-topic replacement | books-route-all-event-types: independent source invocation count |101/0|

## Scope and evidence

Native bodies keep source operation order: track before disposed checks; socket assignment before listeners; reset/subscriptions before optional reconnect; own-topic key snapshot and JavaScript numeric-key ordering; callback-major candle grouping then USD-equivalent normalization; raw unnormalized trade/book symbols; first trade only; post-delivery deduplicated statistics. Candle/message for-of loops and event for-of loops stay live, while trade/book callback forEach captures length. Frames/trades/book events retain original Rc roots; candles retain shared mutable roots. Failure handles preserve identity and stop delivery before statistics.

The live oracle extracts eight actual source bodies, source handler/topic initializers, BASE_WS_URL, imported jsonParse and product-ID normalizer. Content-pinned 28-case floor covers scalar fields, lifecycle actions/payloads, errors, malformed/falsy inputs, disposed/no-op paths, update/snapshot/unknown routing, alias differences and inherited ordinary-object keys. Four new native tests assert decoded scalars/UTF16/f64 bits, enum/callback types, payload fields and identities directly; no decode/re-encode comparisons. Seven additional controlled live-source scenarios cover candle identity and surrogate numeric prefixes, message live callbacks/shared roots, bounded trade/book callbacks, live appended trade events and live OHLCV callbacks. Counts/expected output come from literals/live TypeScript, independently of native iteration.

Dense declared candle/trade/book event DTO arrays, UTF-8 product identifiers and plain parsed frames are this slice's boundary. Host methods expose the existing BaseWebSocket/store/config/imported JSON.parse capabilities at source call sites. This does not implement the imported JSON parser or full transport/socket runtime. Data, full CoinbasePublicWebsocket class and the two listener constructors remain pending; exotic/sparse/getter/callable DTOs are outside this slice. No shared type field or shared ExchangeRuntime method changed.

The decoder-inventory test first failed **101** at `5411ec06babe1130a9a1123267174bf1f880c543` for the new host-state constructor, then **101** at `af44fe0717` for the three native callback/listener enums. Exact helper classifications were added without a blanket file exemption; final gate at the SHA above passes. Earlier failed records remain tracked and are not reported as passes.

Tomllib census stops after16 Coinbase manifests /426 rows: **360/17/49 → 368/17/41**, **8 pending→ported, 0→divergent**. This is the authoring-parent comparison; latest integration-relative census and merge preservation will be recorded before push. Static sub-agent review found no remaining blockers in the declared boundary; reviewer ran no Cargo gates.

Not run: full Coinbase suite, workspace builds/tests, host clippy, wasm clippy/build, CI, live HTTP/websocket, full class/listener construction, native imported JSON parser implementation.


## Integration merge preservation

Actual fetched integration `5911167261853a8cbcc99d79cf45b785bb016648` merged at `4c356a3de476f176ffd36bcddc412c995af4768b`. Sole conflict `parity/exempt.toml` resolved by retaining all839 integration entries and appending all27 real Coinbase helper exemptions. Coinbase source/tests plus dependency sources/manifests/lock and live oracle source inputs versus gate SHA `1e3de14942ccb7d0c2730cf18f0cca02a7e4f9df`: scoped diff **0**. Oracle/control scripts versus corrected-control SHA `3e9a35206e65f18bf6e0ec7f2a8a4af94220a8b4`: diff **0**.

Scoped check14tests and the four compiled controls carry across unchanged inputs; they were not rerun after merge-down. Merged extractor/mapping were remeasured at the actual merge SHA: **0/0**, zero extractor updates and426 rows/659 public native items. Direct integration-relative tomllib census remains **360/17/49 → 368/17/41**, **8 pending→ported, 0→divergent**, with every moved row's real committed function verified. Exact preservation commands, exits and census are in `PUBLIC_STREAM_MERGE_GATE_RESULTS.json`. Subsequent delivery commit changes evidence only.


Final fetch advanced to integration `91bdde133d2f407dffa8f2140f94e87eac7c1d5f`, merged at `f1aabc09b394add73a3c2575cf5fa3a412bc5804`. Exemption conflict again preserves all872 integration entries plus27 Coinbase helpers. Source/tests/dependency/live-source diff versus `1e3de14942ccb7d0c2730cf18f0cca02a7e4f9df`, corrected control/oracle scripts versus `3e9a35206e65f18bf6e0ec7f2a8a4af94220a8b4`, and Coinbase manifests/extractor code versus prior merge: **0/0/0**. Cargo/test/control exits carry across these unchanged inputs; they were not rerun at this final merge. Extractor/mapping remeasured at the final merge: **0/0**, zero updates /426 rows /659 public items. Direct latest-integration tomllib census: **8 pending→ported, 0→divergent;368/17/41**. `PUBLIC_STREAM_FINAL_PRESERVATION.json` attaches final preservation and metadata exits to this exact SHA. Delivery evidence commit changes only these receipt files.

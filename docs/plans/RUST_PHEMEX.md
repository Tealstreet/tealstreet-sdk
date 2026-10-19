# Phemex perpetuals Rust port

Branch: `feat/rust-phemex`, based on `origin/feat/rust-rewrite`.

## Plan

1. Public: typed streaming products/tickers, packed ticker updates, sequence-aware snapshot/delta books, candles, connection watchdog. Review and commit.
2. Private: exact HMAC signing, authenticated aop_p stream, independent USDT/USDC account slices, per-market mode/settings even when flat, stale snapshot guards, execution deduplication. `Triggered` maps Closed but is never terminal. Review and commit.
3. Trading: g-API PUT/DELETE sequenced jobs, preflight whole command and caller IDs, bracket/trailing formatting, cancel/amend/settings with acknowledgement guards and no write retries. Review and commit.
4. Queries: bounded typed paged reads and explicit unsupported contracts where venue/TS lacks a safe route; candle reads. Review and commit.
5. Harness/gates: wasm/SDK registration, strict signed-request and WS audit, documented private tape and recorded public fixture, known TS fixes, Binance regressions and all CI checks. Review and commit.
6. Landing: merge latest landing branch, rerun gates, push and PR into feat/rust-rewrite, post adversarial reviews, fix until clean, merge only green CI, cut fresh follow-up branch.

## Plan audit

USDT/USDC PerpetualV2 g-API uses Rp/Rv values. Legacy Ep/Er scaling is handled only where the TS wire contract needs it. Order snapshot is WS-owned; REST position polling must not erase orders. Position mode must survive zero-size account rows. Failed currency reads retain their own prior slice. Every socket requires server.ping and bounded reconnect/watchdog. No credentials or live private writes are needed for gates.

## Progress

- Read current landing brief/spec and Phemex runtime guidance. Full-auto user authorization overrides the plan skill confirmation pause.

- Public initial native tests pass (4), typed market/ticker/candle/book implementation and wasm/SDK registration prepared. Root plan audit added malformed-row rejection, bounded book snapshot deadlines, native-symbol preservation and generation fences.
- Private/trading/query implementations prepared in disjoint files with independent tests and cross-review in progress. Query policy uses documented textual `/api-data/g-futures/{trades,orders}` schemas; generic income remains an explicit unsupported result pending verified ledger semantics, matching the absence of a safe TS capability.
- Public live recording exposed a real engine limitation: `/public/products` is 2.54MB with large unrelated arrays beside `perpProductsV2`, exceeding the 1MiB metadata cap. A bounded opt-in sibling-list omission mode is being implemented and independently reviewed; the memory limit will not be raised.
- Harness now separates authenticated/private and public sockets sharing a URL, validates exact REST/WS signatures and subscription order, and explicitly bootstraps only flagged initial auth/snapshot records. Harness tests 17/17 pass.

### Phase reviews and first gates

- Public reviewed independently by private_impl; fixed strict two-sided REST depth validation, positive product precision, monotonic 429 backoff, native candle symbols. Added bounded `PathOmitLists` streaming for the 2.54MB catalog; engine 24 tests pass and root reviewed parser changes.
- Private reviewed by trading_impl; fixed mutation of caches on malformed snapshots, permanently blocked amendments after fills, and stale currency balances. Account 14 and signing 2 tests pass. Triggered→Closed remains explicitly nonterminal.
- Trading reviewed by public_review; fixed unready/disconnected cancel-all admission and settings ACK/context races. 23 tests pass, including real-value PUT routing, caller-ID guards, Init acknowledgements, partial cancel/fill races, brackets and exact split sizes.
- Queries reviewed by trading_impl; fixed historical close-on-trigger reduce-only semantics, zero-sized close-entire stops, and trailing metadata. 12 tests pass. Generic income is explicitly unsupported pending verified ledger scope/sign semantics.
- Live full-depth public fixture: 20 seconds, 533 replayed frames, 891 markets, 324 tickers, 2 books. First passing median gate: TS/Rust CPU 277.9/90.9ms, steady 178.4/30.6ms, retained 5.8/2.8MB. Further final gates follow landing merge.
- TS bugs fixed and recorded in KNOWN_TS_FIXES: implicit depth30 subscription leaves deep levels stale; lost outer sequence discards buffered deltas; periodic full snapshots must replace old levels; zero-open percentage must remain finite; fills deduplicate by execution rather than order/message; cumulative executed exposure survives cancellation; exact split lots and partial-fill replacement remain bounded and fail closed.
- Crypto audit: this port reuses only existing HMAC/SHA/UUID primitives and replaces no currently called crypto helper; no additional provably unused helper introduced.

### Landing integration checkpoint

- Merged BingX landing head `9ddec5d0a4`. Independent merge reviews preserve incoming selectors, elapsed memory quiescence, strict write/subscription gates, and all Phemex hooks.
- Engine 36 tests pass after combining catalog omission and incoming selector aliases. WASM registers all seven venues. The Phemex public gate passed before integration; final full matrix will run on the combined artifact.
- Private first run found TS drops flat ETHUSDC leverage settings; the independent taped-row oracle now restores that known setting, cumulative fills, and per-execution fills without referencing Rust output.
- Harness review fixed unsigned history reads accepted by audit, missing full-depth subscription validation, and ambiguous shared-endpoint close records (fail closed). Added independent oracle regression and signature/depth tests.

- Combined seven-venue artifact after BingX integration: 2,923,610 bytes. Aggregate static budget becomes 3MiB (3,145,728 bytes, 7.6% headroom); CPU/memory comparison remains strictly Rust<TS. Final Phemex gate medians so far: 219.0/73.0ms CPU, 141.7/20.5ms steady CPU, 5.7/2.8MB retained; private 14 frames PASS.
- Full source workspace fmt/clippy/tests and wasm clippy in progress; root typecheck 44 tasks, lint 28 tasks and Phemex TS 39 tests pass. Source Rust suites: public6/private14/signing2/trading23/queries12 =57 tests.

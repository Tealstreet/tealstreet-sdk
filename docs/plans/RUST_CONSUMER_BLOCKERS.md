# Phase 9 consumer blockers

Base: `origin/feat/rust-rewrite` (`a7caec9314`), branch
`feat/rust-consumer-blockers`. Full auto, no TypeScript venue deletion or
registry-default change. Follow VENUE_PORT_BRIEF gates and independent reviews.

## Phases

1. **Shared public feed.** Engine-owned direct/disabled/shared policy suppresses
   public-role sockets, including reconnects, while private/business sockets and
   native metadata discovery remain independent. `privateOnly` maps to disabled.
   A worker-safe MessagePort provider fans out typed public tickers, books and
   candles into native state, preserving account settings and private state.
   Reject legacy TS `externalPublicData` on Rust with migration guidance rather
   than silently ignoring it. Test real private worker/provider fan-out,
   subscriptions, disposal, identity mismatch and native snapshots. Audit scope:
   role isolation, private-data exclusion, native state versus JS overlays,
   readiness and cleanup races.
2. **Public factory lifecycle.** Start native public engines explicitly on a
   metadata miss. Forced refresh starts a new instance and atomically replaces
   the handle after successful fetch/cache write, retaining good state on error.
   Test real native cold/refresh requests, cached hydration, concurrent calls and
   disposal. TS semantics stay compatible.
3. **Real consumer loading.** Prove saved-account CLI and MCP calls reach actual
   native stores; verify packaged Node/Bun artifacts and host-injected mobile
   loading without browser fetch assumptions. Distinguish real device/runtime
   support from a compiled-WASM loader test. Update nearby runtime docs.
4. **Readiness and gates.** Update RUST_REWRITE and RUST_CUTOVER_CALLERS with exact
   closed/remaining boundaries. Run root typecheck/lint/tests, SDK WASM tests,
   Rust bans/fmt/Clippy/native tests, host/harness tests and Binance plus each
   protocol-touched venue gate. Direct-mode venue payloads remain unchanged;
   Hyperliquid multiplexed public subscriptions are gated by ownership policy. Review each
   phase independently, fix findings and commit checkpoints.
5. **PR correctness and landing.** Push, PR into feat/rust-rewrite, independent
   code-correctness reviews posted as comments, fix/repeat until passing or
   diminishing returns. Integrate landing drift, rerun affected gates, wait for
   exact-head green CI, merge and cut fresh follow-up branch.

## Design boundaries

The shared provider owns public subscriptions; consumers retain venue-native
metadata discovery/indexes/account overlays and all private REST/streams. Feed
updates are typed public fields only, mapped by symbol into each engine. A shared
feed never turns a private account into a public account or authorizes trading.
Cross-worker ports require explicit transfer. A missing provider never silently
falls back to direct public sockets. Feed identity includes venue/network/product
options; incompatible feeds fail before account readiness, after native product discovery.

The factory handle's `exchange` may change after forced refresh. A cache hit is
metadata hydration, not proof a live feed has started. Refresh failure must not
publish an empty snapshot or dispose the previous good engine.

## Progress

- Initial architecture/plan inspected; three disjoint implementation scopes
  assigned (native policy, factory, consumer loading), SDK provider owned by root.
- Plan audit: native store ingest is required; copying shared public JS fields
  into the account store would bypass native pricing/frame consumers. Fresh
  instance refresh avoids adding incompatible per-venue refresh state machines.

- Resume checkpoint: WIP `76e3fb9570` preserves all implementation before merging
  landing `0063068241`; no PR existed. Native policy phase independently reviewed
  PASS after fixing periodic REST overwrite and unknown/empty seed readiness.
- Factory review fixes: transactional lazy-cache refresh, UTC-bucket-aware
  coalescing and disposed fallback protection. Verification: 32 unit and 7 native
  WASM cases, typecheck/lint PASS.
- Consumer review fixes: compiled Bun embedding, packaged Node WASM/CJS assets,
  real MCP balance capability, Rust binary dependency notices, GitHub-owned-only
  workflow steps and exact package manifest checks. Mobile seam tested with real
  bytes under Node; Hermes/native bridge/device remains explicitly unproved.
- SDK review fixes: started/loaded provider requirement, native catalog/contract
  comparison, matched shared-data startup latch, fail-stop provider loss and
  heartbeat cleanup. Later catalog changes also require restart. Shared worker
  test fixture must supply Web Crypto on the SDK test environment's fake window;
  production Node loaders have the real runtime crypto.

- Native multiplex review found Hyperliquid uses a private-role socket for both
  public and private topics. Non-direct ownership must suppress public topics,
  retain private auth/pings/reconciliation and recompute existing native derived
  balances from provider quotes. Hyperliquid joins Binance/Coinbase/Phemex local
  replay controls; direct mode remains unchanged.
- Final broad checks underway after frozen install relink completed (542s).
  Root lint: 28 tasks PASS. Initial root typecheck: 43/44 tasks PASS; the SDK
  failure was two new fixture typing errors (old diagnostic listener and wrong
  balance property), corrected before rerun. Do not classify this as final green.

- Consumer phase reviewed PASS and committed `9ec311e98f`: CLI 2 WASM tests,
  MCP 2 protocol-WASM + 29 unit tests, RN 2 WASM/native-registry tests, host 70,
  package build/isolated smoke and scoped typechecks/lints PASS. RN registration
  is awaited and import-neutral; actual frame I/O requires codec globals.
- Factory phase final verification: 37 unit and 9 native WASM tests PASS,
  including held metadata transport deadlines, cached lazy transactions and
  shared-port option rejection. Shared SDK suite: 6 WASM cases PASS; catalog
  invalidation uses an explicitly independent binary SDK-boundary fixture.

- Resume validation: SDK unit rerun 7,328/7,328 in 505 files; combined-WASM
  suite 382/382 in 12 files, plus the real AsterDEX canonical candle alias
  regression (1/1). Root typecheck 44 tasks and lint 28 tasks PASS; other
  workspace unit suites 25 tasks PASS. Independent review also caught MCP quote
  freshness: Rust must keep native public feeds because its read methods return
  engine state; the TS-only socket-disable hook remains on TS accounts.
- Factory recovery review added transactional stale-fallback/current-cache
  revival after timed-out cold starts and metadata revision fences for delayed
  reads. Current focused verification: 38 unit + 10 native WASM cases PASS.
- Native broad verification: 2,379 tests across 192 result suites, harness
  533/533 and JS host 70/70 PASS. A Direct-mode SocketId(0) fixture compatibility
  regression was caught and fixed by limiting sentinel suppression to non-direct
  modes. Hyperliquid shared quotes revalue an already seeded private balance;
  they cannot fabricate private startup readiness.
- Landing advanced to TxFlow PR #1329 during verification; commit checkpoints
  precede integration, then mixed-socket ownership and combined artifact gates
  are repeated against the integrated tree.

- Integrated TxFlow landing `bf1cf3c99a` in `042d4e4e9f`, preserving new Rust
  registry/query contracts and the repaired factory readiness row. Runtime
  inventory is now 49 registered names / 44 Rust aliases / 9 defaults, as
  established by the landing lane; this work changes no additional defaults.
- Final cache-race regression passes both outer lookup and locked refresh lookup
  recovery after failed cold startup (11 native WASM cases). SDK/shared, native,
  factory and consumer checkpoints committed separately before landing sync.

- Integrated SDK verification PASS: 7,411 unit tests in 512 files; root
  typecheck 44/44 tasks and lint 28/28 tasks. A new candle fixture now explicitly
  models a JavaScript caller passing an unsupported interval into the existing
  Aster fallback, without broadening the TypeScript interval contract.
- Additional code-correctness review of native price consumers found direct
  quote handlers revaluing private PnL in GRVT, Lighter and ApeX Omni. Shared
  ingress must invoke those same native formulas through the venue hook, as
  Hyperliquid already does. Dedicated native regressions and all three venue
  replay controls are added before the final artifact/gates. Binance Spot/COIN-M
  have no separate public-price cache: their existing private/REST reconciliation
  cadence uses Store prices in both modes. The targeted quote-side-effect audit
  found no further concrete bypasses; this is code audit evidence, not all-venue
  live connectivity proof.

- Native follow-ups committed after independent reviews: TxFlow `558dc48f5b`
  (76 tests), ApeX Omni `94e734dc31` (101 tests plus final two hook regressions),
  GRVT/Lighter `868340d9a5` (177 tests), strict Clippy PASS. Explicit zero-mark
  regressions preserve each venue's normalized mark-to-last fallback; cumulative
  shared snapshots intentionally retain the current mark on last-only updates.
- Final combined artifact at `868340d9a5`: 11,387,205 bytes; web/node and packaged
  MCP SHA-256 `0adfe0dcc81bfc2af5f47bf5c6189cd865416be8713decfaea11e5c4474f313c`.
  SDK WASM 391/391 in 14 files, server native WASM 11/11, CLI 2/2, MCP 3/3,
  RN loader 2/2 PASS. MCP final build and isolated package smoke PASS: 17 protocol
  tools, Poseidon and actual native Coinbase startup outside the checkout;
  131 JavaScript and 353 Rust dependency notices.

- Integrated companion server-adapter PR #1334 (`c10f7e190a`) as `e8c62f4d23`.
  Factory APIs stay unchanged; the adapter delegates the exact stable handle,
  while its native session path bypasses TS creation. The only native addition
  is a typed offline fixture example, so the verified production WASM checksum
  remains valid. Full integrated Rust workspace: 2,463 tests / 202 suites PASS;
  native session example/integration and expanded server tests are checked next.

- Final store-contract audit found unrelated book/candle packets forcibly replayed
  quote valuation, producing false position update IDs in venues whose existing
  position mutator marks the section dirty. Shared-only fix `12d8c71421` forces
  restoration only for accepted known-symbol ticker values; pending native
  ticker revisions still restore provider authority. Both independent reviews
  PASS. Prove-fail: old force flag failed the book-only hook count; corrected
  engine all-feature suite 81 tests and strict Clippy PASS.
- Fan-out proof uses actual `ExchangeWorker` instances, real MessagePorts and
  structured-clone transfers in Node; it is not a separate browser-worker
  process or deployed-gateway acceptance test.

- Initial Direct replay controls passed 12 public timing/memory gates and 19
  private correctness gates. Final bindings with the Shared-only store fix are
  11,388,401 bytes, SHA-256
  `afeee9aae01738d58881552b52e5f64579c8818e0b1208d8d9329636f4f6d710`.
  Full current Rust workspace 2,464/202 suites and strict WASM32 Clippy PASS.
  Current-asset CLI/MCP/RN seven tests, server eleven WASM cases and isolated
  MCP package build/smoke PASS; packaged bytes match that checksum.
- The full SDK rerun exposed a fixture freshness timing error: the first worker
  JWT was checked after another startup and REST polling, beyond the oracle's
  strict five-second issuance window. `221b826cb6` verifies actual packets at
  send time using the unchanged independent cryptographic oracle, retaining the
  result for later assertions. Independent review, six shared cases, SDK types
  and explicit file lint PASS. A full SDK rerun and final-artifact replay batch
  follow; public timing gates run alone, private correctness cases use two
  independent processes at a time.

- Final full SDK rerun after the fixture correction PASS: 391/391 in 14 files
  (29.19s). Final native workspace 2,464 tests / 202 result suites, current
  wasm32 strict Clippy, seven consumer tests and eleven factory WASM cases
  PASS on `afeee9aa…6d710`. The final-artifact replay batch passed all 12 public and 19 private controls, with
  three-run public medians sequentially and one-run private correctness in
  pairs; no builds or other owned checks run during public measurements.

- Integrated landing `02e9db0e8b` (GRVT replay indexing/product decision and
  Lighter Robinhood variants) as `5b51d8875b` after committing the completed
  gate evidence. Readiness now tracks 49 names / 45 Rust aliases / 9 defaults /
  36 opt-in names / 4 unavailable names. Preserve the explicit GRVT retirement
  decision; this lane changes no product/default selection. Shared Lighter
  policy integration and fresh combined-artifact affected gates follow.


## Final public replay metrics

Three-run medians on the final `afeee9aa…6d710` combined binary. Values are
TypeScript → Rust. Public gates enforce main-thread CPU, steady CPU and
retained memory; process CPU is an additional diagnostic. Private replay
controls enforce correctness only. Existing documented fixtures are reused;
this consumer change adds no live venue capture or new TS protocol correction.

| Profile | Main CPU ms | Steady CPU ms | Process CPU ms | Retained MB |
| --- | ---: | ---: | ---: | ---: |
| binance | 179.8 → 92.6 | 98.1 → 33.8 | 240.6 → 164.1 | 18.5 → 5.2 |
| binancespot | 517.2 → 385.3 | 225.8 → 111.5 | 690.1 → 529.3 | 27.9 → 11.8 |
| binanceinverse | 161.3 → 69.0 | 108.8 → 35.7 | 191.9 → 106.8 | 14.2 → 3.0 |
| binancepm | 170.7 → 93.0 | 95.2 → 34.0 | 225.9 → 164.4 | 18.4 → 5.2 |
| hyperliquid | 342.6 → 59.3 | 23.2 → 20.8 | 431.6 → 149.6 | 24.7 → 3.8 |
| coinbase | 804.4 → 310.2 | 464.8 → 87.2 | 1012.5 → 426.0 | 39.1 → 21.7 |
| phemex | 369.7 → 113.9 | 255.3 → 40.0 | 463.2 → 186.0 | 20.5 → 5.6 |
| asterdex | 174.0 → 99.4 | 65.2 → 45.3 | 299.7 → 210.9 | 28.2 → 7.5 |
| txflow | 74.8 → 55.1 | 13.0 → 9.3 | 119.3 → 100.2 | 17.9 → 2.6 |
| grvt | 168.3 → 153.6 | 111.4 → 94.9 | 202.2 → 255.9 | 27.2 → 2.8 |
| lighter | 291.5 → 177.4 | 191.3 → 103.1 | 386.3 → 301.8 | 17.6 → 3.9 |
| apexomni | 78.1 → 36.6 | 47.1 → 6.1 | 109.3 → 81.0 | 21.8 → 3.4 |

All 12 public controls PASS. GRVT process CPU is higher in this run;
its gated main/steady CPU and retained memory remain below TS. Public
main-thread speedups range from 1.10× to 5.78×; retained memory reductions
range from 1.80× to 9.58×. No benchmark builds/tests ran concurrently with
these public measurements in this lane.

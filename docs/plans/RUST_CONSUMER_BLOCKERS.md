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


- Lighter integration checkpoint `eb9b96a2de`: strict fixture handles the new
  authenticated tier request; mainnet/testnet Robinhood × disabled/shared policy
  tests preserve private sockets, native USDG metadata, private snapshots and
  provider-derived prices/PnL across polling. Independent review PASS; full
  Lighter suite 107 tests. A central run compiled before the fixture edits settled
  and executed an obsolete assertion; fresh default/all-feature focused runs and
  the full workspace rerun use the committed fixture.
- Integrated artifact is 11,395,355 bytes, SHA-256
  `eb2f1751aeb243ccf46264aa35b1a2e2e99de17c6f03463cc2f40a37594b3359`.
  SDK unit 7,457/513 files, SDK WASM 413/14 files, host 70, harness 561,
  server unit 138 and native WASM 11, and CLI/MCP/mobile seven WASM tests PASS.
  MCP isolated build/smoke PASS with matching packaged bytes and unchanged
  131 JS / 353 Rust dependency notices. Root typecheck 44 and lint 28 tasks PASS.
  Central native workspace: 2,487 tests / 205 result suites PASS; workspace
  all-target/all-feature Clippy, WASM32 Clippy, bans and formatting PASS.
  Current-artifact replay follows. This adds Robinhood public/mainnet/testnet
  controls to the prior twelve public/nineteen private control set.

- Integrated final replay batch PASS: 13 public timing/memory controls and 22
  private correctness controls on `eb2f1751…b3359`, including all four Binance
  profiles, every protocol-touched venue, Coinbase INTX/CFM/spot × ES256/EdDSA
  plus its spot-only alias, and both Lighter deployments on mainnet/testnet.
  Public timing runs were sequential with all other lane checks quiet; private
  correctness ran in pairs. Landing remained `02e9db0e8b` at final fetch.
  All local phase gates/reviews pass; PR correctness review and green CI follow.

- Opened [PR #1342](https://github.com/Tealstreet/tealstreet-sdk/pull/1342)
  into `feat/rust-rewrite`; phase findings and fresh review findings are posted.
  Native policy/hosts/valuation review PASS (22 focused native + 2 WASM ingress
  cases). SDK review found equal-seed readiness not waking and provider-owned
  book/candle callbacks being suppressed on frame-forwarding sources; both
  regressions prove-fail before `6392a068d1` and pass after it. A separate
  reviewer confirms callback isolation/refcounts, Aster clones and buffer use
  before transfer. Full SDK WASM now 414/14 files; types/lint PASS.
- Consumer review found MCP's TS Infinity override disabled BreakoutProp's
  native REST polling. `f29b10eb7e` applies the TS policy only after actual
  instance selection, preserving native defaults/opt-in and caller options.
  The final real-MCP/actual-WASM changed-quote regression fails with the old
  policy and passes with the fix. All five MCP WASM + 29 unit tests,
  typecheck/lint/build/isolated smoke PASS; two CLI + two mobile proofs make
  nine consumer WASM tests. Independent consumer re-review PASS.
- CI exposed Paradex solo bundle growth to 1,290,381 bytes over its unchanged
  1,258,291-byte limit. A Paradex-only host size profile produces 1,221,468
  bytes with engine hot loops still O3; default/all other 33 selection argument
  arrays remain unchanged. Independent build-script review PASS. Official
  solo public/private replay gates PASS; combined web/node artifacts restore
  exactly to `eb2f1751…b3359`.
  No size-budget increase or native semantic change is used.

- Local follow-up gates PASS: BreakoutProp and Paradex public/private controls,
  bringing final coverage to 15 public CPU/memory and 24 private correctness
  controls. The final MCP polling test also proves failure with the old Infinity
  override using its corrected time-gated HTTP fixture, then restores committed
  production code. Final source root typecheck 44/lint 28 and SDK 414 WASM pass.
  No native venue logic or combined WASM bytes changed in this PR review round.

- Landing advanced to spot-products PR #1339 (`f4ea9e06ab`) during the PR review
  round. Integrated as `20e7a6d401`, retaining the new account-validation
  startup/signer guards plus all reviewed consumer fixes and the scoped Paradex
  build profile. Current inventory is 49 names / 47 Rust aliases / 9 defaults /
  38 opt-in / 2 absent (Bitunix and KuCoin Spot).
- New spot integration audit: Hyperliquid's existing spot guard prevents perps
  valuation; both spot ports intentionally value holdings at private balance
  application, not quote-only events. Shared quotes must retain that cadence
  while remaining authoritative for the next private apply. Test-only native
  ownership/reconnect/price-restoration evidence follows; no new spot price
  cache or quote-time valuation is introduced. The combined binary and affected
  consumer/gate proofs are rebuilt for this integration before final CI/landing.

- Spot integration checkpoint `da128ffc37` adds test-only native coverage;
  focused 36 tests and independent review PASS. Full merged native workspace
  2,558 tests / 210 result suites, workspace all-target/all-feature Clippy,
  WASM32 Clippy, bans/fmt PASS. Merged SDK unit 7,482/516 files, SDK WASM
  426/14 files, server 138 unit + 11 native WASM, host 70/harness 571, spot frame
  integration 8 and all nine consumer WASM cases PASS. Root typecheck 44 and
  lint 28 tasks PASS; packaged isolated smoke retains 131 JS / 353 Rust notices.
- Current combined web/node/packaged artifact is 11,614,314 bytes, SHA-256
  `d1fdda29b8593918707278e5371419e19d6a7be73d167b70c3a8f8a8668e7b88`.
  Final combined replay covers 17 public/26 private profiles, including both
  spot aliases and a Kraken futures control; Paradex solo adds one of each.
  Public timing runs alone after all other owned checks stop. Final metrics
  replace the pre-spot measurements below once the replay completes.

- Runtime diagnostic checkpoint: the 67-frame Hyperliquid Spot fixture missed
  main/steady CPU on the combined Node 24 build (38.8/39.7 ms main) and main CPU
  on its canonical O3 solo build (36.7/38.7 ms). A compact-host experiment still
  missed Node 24 main CPU and was discarded; no Hyperliquid profile, fixture or
  threshold change is used. The unchanged canonical O3 artifact passes on CI's
  Node 22.23.3: main 39.8/35.5 ms, steady 11.5/9.9 ms, retained 18.7/6.0 MB.
  Private correctness passes. Keep this Node 24 short-fixture performance
  limitation visible; the actual-byte consumer functional proofs pass Node 24.
- Final performance controls are standardized on CI Node 22.23.3 instead of
  mixing Node 24 trials into the acceptance table. Hyperliquid Spot, Kraken
  Spot/futures and Paradex use their canonical solo artifacts; all four public
  and private gates PASS. The combined artifact restores exactly to `d1fdda29…8e7b88` before remaining public/private controls. No benchmark tests/builds run
  concurrently during accepted public measurements.

- Final local acceptance PASS on the integrated/reviewed source: 18 public
  CPU/memory controls and 27 private correctness controls, all measured on
  CI Node 22.23.3. Fourteen public/23 private controls use the checksum-verified
  combined artifact; four public/private solo profiles cover Hyperliquid Spot,
  Kraken Spot/futures and Paradex. All three PR correctness findings and the
  Paradex size finding are fixed, independently re-reviewed and closed. No TS
  protocol bug normalization is added. Exact-head CI and merge into
  `feat/rust-rewrite` remain the final landing steps.

- CI cold-cache checkpoint: the Binance job reached MCP license generation
  after passing its runtime tests, then Cargo metadata failed under `--offline`
  because a locked workspace/dev manifest was absent from the production WASM
  build cache. License generation now permits Cargo to fetch missing sources,
  retaining `--locked`, the WASM platform filter, the non-dev notice closure
  and the existing license allowlist. No dependency, lockfile or runtime asset
  changes. Exact CI Node 22.23.3 MCP build, 29 unit tests, typecheck, lint and
  isolated smoke PASS; notices remain 131 JS / 353 Rust and the packaged WASM
  checksum is unchanged. Independent review and fresh exact-head CI govern
  landing; native replay controls remain valid for this build-only fix.

## Final public replay metrics

CI runtime: Node 22.23.3, three-run medians. Fourteen profiles use the current
`d1fdda29…8e7b88` combined binary; Hyperliquid Spot, Kraken Spot/futures and
Paradex use canonical single-venue builds. Values are TypeScript → Rust.
Gates enforce main-thread CPU, steady CPU and retained memory; process CPU is
an additional diagnostic. Private replay controls enforce correctness only.
Existing documented/live fixtures are reused; this lane adds no live capture,
TS protocol correction or threshold change. Node 24's short Hyperliquid Spot
CPU miss is recorded above and is not represented as a passing control here.

| Profile | Main CPU ms | Steady CPU ms | Process CPU ms | Retained MB |
| --- | ---: | ---: | ---: | ---: |
| binance | 199.7 → 95.4 | 109.7 → 38.1 | 255.8 → 155.5 | 18.2 → 4.8 |
| binancespot | 583.0 → 381.8 | 247.7 → 113.5 | 869.6 → 572.5 | 27.6 → 11.7 |
| binanceinverse | 165.8 → 65.8 | 108.7 → 35.5 | 199.3 → 95.2 | 13.8 → 2.7 |
| binancepm | 191.8 → 92.4 | 107.5 → 36.5 | 248.6 → 152.2 | 17.9 → 4.8 |
| hyperliquid | 323.9 → 50.0 | 26.4 → 14.7 | 445.1 → 134.1 | 24.3 → 3.4 |
| hyperliquidspot | 39.8 → 35.5 | 11.5 → 9.9 | 69.5 → 86.9 | 18.7 → 6.0 |
| krakenspot | 248.6 → 120.2 | 186.7 → 67.0 | 368.2 → 264.5 | 16.5 → 4.3 |
| kraken | 963.7 → 167.7 | 703.0 → 93.8 | 1131.4 → 316.6 | 16.8 → 2.7 |
| coinbase | 835.0 → 338.4 | 499.6 → 110.1 | 1134.5 → 462.7 | 38.5 → 21.4 |
| phemex | 392.8 → 102.5 | 264.6 → 35.0 | 469.0 → 157.8 | 20.1 → 5.3 |
| asterdex | 165.7 → 88.7 | 60.1 → 39.3 | 280.6 → 168.6 | 27.9 → 7.0 |
| txflow | 109.2 → 64.8 | 20.7 → 12.6 | 151.9 → 101.8 | 17.3 → 2.2 |
| grvt | 175.8 → 121.2 | 95.1 → 67.0 | 276.4 → 270.8 | 26.5 → 2.6 |
| lighter | 308.0 → 194.2 | 194.6 → 109.6 | 460.2 → 370.4 | 17.3 → 3.7 |
| lighter-robinhood | 240.4 → 85.1 | 165.9 → 36.9 | 307.1 → 183.3 | 19.9 → 5.8 |
| apexomni | 81.3 → 34.8 | 48.8 → 5.6 | 118.0 → 72.5 | 21.3 → 3.2 |
| breakoutprop | 16.4 → 16.0 | 6.5 → 4.3 | 16.6 → 20.1 | 11.9 → 1.7 |
| paradex | 530.1 → 477.8 | 231.9 → 173.9 | 723.3 → 602.5 | 19.0 → 2.1 |

All 18 public controls PASS. Process CPU is higher for hyperliquidspot, breakoutprop;
their gated main/steady CPU and retained memory remain below TS. Public
main-thread speedups range from 1.03× to 6.47×; retained memory reductions
range from 1.79× to 10.08×. No builds/tests ran concurrently with these
accepted public measurements in this lane.

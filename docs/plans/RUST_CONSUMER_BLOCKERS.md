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

- Product-semantics integration plan: landing advanced to PR #1341
  (`5e92f93a8a`) during final CI. Merge preserves typed classification,
  derivative terms/Greeks, combined Bybit selectors, OKX products and the new
  cold-subscription path alongside native public-feed ingress and worker
  readiness/callback behavior. Both CI consumer and product-preflight steps
  are retained. Independent merge reviews identified semantic catalog identity
  and selector-specific factory cache isolation as integration fixes; add
  focused regressions and replace the incoming stale legacy-provider test.
  Then rebuild actual combined bytes, rerun native/consumer/store checks and
  canonical replay controls, and obtain fresh exact-head CI before landing.
  Previous artifact metrics below remain evidence for the pre-integration head
  until replaced by the new acceptance checkpoint.

- Integrated source verification: 2,607 native tests / 212 result suites,
  strict native all-target/all-feature and WASM32 Clippy, bans/fmt PASS.
  Cold and non-cold host dispatch both preserve typed public-feed ingress,
  original clocks and single flushes. SDK unit 7,497 / 517 files and WASM
  430 / 14 files PASS; 16 native CLI product controls, 576 harness / 70 host
  tests, nine CLI/MCP/mobile WASM cases and isolated MCP smoke PASS.
  New combined web/node/packaged artifact is 11,826,378 bytes, SHA-256
  `6afa3acf260286cac3be6e8bf0a162fb7cafbca4241e000b5a9d587bc9a94c34`;
  packaged notices remain 131 JS / 353 Rust.
- Shared semantic catalog closure `d99b895044` adds typed classification,
  future and option identity to the public-provider fingerprint without
  transporting metadata into private state. Four boundary regressions prove
  failure against the old fingerprint; production is restored and all checks
  pass. Independent review PASS. Factory product cache partition `ed31745ae4`
  initially covered explicit Rust selection; fresh review found that registry
  defaults must use the actual resolved native class too. Correct that path
  and add default-Orderly testnet isolation before final acceptance.
- Pre-integration head `6d160de6f7` passed all 39 CI checks. Cold-cache packaging
  fetched the missing locked manifest and passed. Hyro's 160-frame steady-CPU
  gate initially missed at TS/Rust 3.2/3.7 ms; the prior run passed at 7.3/7.0.
  Independent review confirmed unchanged runtime/build trees and a different
  runner image. One unchanged-job rerun passed; no fixture/threshold change.
  Fresh integrated-head CI and replay acceptance are still required.

- Final integrated acceptance: 21 public CPU/memory profiles and 36 private
  correctness controls PASS on Node 22.23.3. Fifteen public/32 private controls
  use the actual combined SDK artifact; six public/four private profiles use
  canonical solo builds. All five solo selections remain within the unchanged
  1,258,291-byte budget. The additional local OKX EU solo CPU miss and passing
  combined comparison are retained explicitly below; no native code, thresholds
  or fixtures were changed to resolve that artifact comparison.
- Cache correction `1bad4d499e` uses the resolved native instance, covering
  default Rust selection and actual TS fallback keys. Two default-selection
  regressions prove-fail the old guard; three product-profile regressions
  prove-fail unpartitioned keys. Final server unit 143 / WASM 11, types/lint
  PASS. Independent full integration re-review PASS, no open findings. Root
  typecheck 44 / lint 28 and all fresh artifact consumer/native/store proofs
  recorded above PASS. Fresh exact-head CI remains required before merge.

- Exact-head CI checkpoint `0c494cd355`: 38 checks pass, including Linux's
  canonical solo OKX EU gate (main TS/Rust 2,658.1/2,370.1 ms; steady
  2,009.0/1,795.9; memory 21.6/6.7 MB). Hyro's 160-frame steady-CPU gate
  fails two attempts: TS/Rust 7.3/7.4 ms, then 2.9/3.4; main CPU,
  correctness and memory pass. The one reviewed unchanged-job rerun is spent;
  stop reruns and profile matched canonical solo artifacts/runtime against
  landing. Local emitted-code audit refutes a large restoration prologue;
  investigate measured hot work before choosing a bounded native fix. Preserve
  all Shared authority/readiness, ticker dirty-state and private-role behavior,
  and rerun affected native/consumer/replay checks. Fixtures, thresholds and
  output oracles remain unchanged. Merge stays blocked on this gate.

## Final public replay metrics

CI runtime: Node 22.23.3, three-run medians. Fifteen profiles, including OKX EU,
use the current `6afa3acf…94c34` combined SDK binary; Hyperliquid Spot, Kraken
Spot/futures, Paradex, Bybit and global OKX use canonical solo builds. Values
are TypeScript → Rust. Gates enforce main-thread CPU, steady CPU and retained
memory; process CPU is an additional diagnostic. Private controls enforce
correctness only. Existing fixtures, thresholds and protocol oracles remain
unchanged.

The supplementary OKX EU solo build missed CPU on this Mac/Node 22 run
(main 3,121.0 vs TS 2,637.6 ms; steady 2,252.8 vs 2,117.6), while correctness,
memory and its size budget passed. A controlled comparison of the actual SDK
combined artifact passes (main 2,079.8 vs 2,250.5 ms; steady 1,596.3 vs 1,699.3),
matching landing's combined baseline (Rust main 2,081.7 / steady 1,596.1).
Independent source review found no new Direct-mode scan/allocation/repricing
loop. This is retained as a build/runtime limitation; canonical solo Linux CI
remains a separate required gate. The earlier Node 24 short Hyperliquid Spot
CPU miss also remains documented. Neither miss is normalized away or counted
as a passing trial.

| Profile | Main CPU ms | Steady CPU ms | Process CPU ms | Retained MB |
| --- | ---: | ---: | ---: | ---: |
| binance | 194.1 → 91.4 | 107.0 → 37.1 | 246.1 → 152.8 | 18.1 → 4.9 |
| binancespot | 585.0 → 405.0 | 242.9 → 122.1 | 890.6 → 613.9 | 27.6 → 11.7 |
| binanceinverse | 172.2 → 67.4 | 112.8 → 37.4 | 205.7 → 99.7 | 13.9 → 2.7 |
| binancepm | 261.8 → 139.3 | 156.4 → 71.3 | 308.8 → 207.4 | 17.9 → 4.9 |
| hyperliquid | 442.6 → 75.1 | 44.9 → 31.0 | 524.3 → 179.8 | 24.5 → 3.4 |
| hyperliquidspot | 47.4 → 44.5 | 18.2 → 15.1 | 60.2 → 106.9 | 18.8 → 6.0 |
| krakenspot | 244.4 → 133.0 | 188.9 → 72.4 | 364.6 → 277.7 | 16.5 → 4.3 |
| kraken | 979.5 → 165.6 | 743.6 → 90.3 | 1091.6 → 304.1 | 16.9 → 2.7 |
| coinbase | 909.6 → 390.4 | 554.4 → 123.8 | 1121.0 → 489.4 | 38.5 → 21.5 |
| phemex | 468.3 → 155.7 | 318.8 → 70.0 | 520.9 → 220.4 | 20.1 → 5.3 |
| asterdex | 254.3 → 142.3 | 112.2 → 77.0 | 369.9 → 245.2 | 28.1 → 7.1 |
| txflow | 131.1 → 88.3 | 35.9 → 23.9 | 170.0 → 129.1 | 17.4 → 2.2 |
| grvt | 137.6 → 121.3 | 80.7 → 69.1 | 233.2 → 268.4 | 26.6 → 2.6 |
| lighter | 313.3 → 212.8 | 200.9 → 113.8 | 467.3 → 400.7 | 17.4 → 3.7 |
| lighter-robinhood | 329.8 → 119.6 | 228.5 → 59.5 | 400.5 → 231.7 | 20.0 → 5.8 |
| apexomni | 95.9 → 40.0 | 57.7 → 8.2 | 111.0 → 84.7 | 21.3 → 3.2 |
| breakoutprop | 26.2 → 17.5 | 10.9 → 6.8 | 28.3 → 22.5 | 11.9 → 1.7 |
| paradex | 558.0 → 457.0 | 263.0 → 168.8 | 720.1 → 575.8 | 18.9 → 2.1 |
| bybit | 181.2 → 141.9 | 86.3 → 49.1 | 286.6 → 338.0 | 22.2 → 5.0 |
| okx | 1121.0 → 998.2 | 841.1 → 743.2 | 1391.0 → 1252.9 | 19.3 → 5.3 |
| okxeu | 2250.5 → 2079.8 | 1699.3 → 1596.3 | 2534.6 → 2375.6 | 21.8 → 7.6 |

All 21 accepted public profiles PASS on the artifacts identified above.
Process CPU is higher for hyperliquidspot, grvt, bybit; their gated main/steady CPU and
retained memory pass. Main-thread speedups range from 1.07× to
5.91×; retained memory reductions from 1.79× to
10.23×. Public measurements ran sequentially with no other owned
build/test processes. All 36 private controls PASS. Combined browser/Node
bindings restore exactly to SHA `6afa3acf…94c34` after solo trials.

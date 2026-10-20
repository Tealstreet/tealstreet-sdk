# Rust rewrite audits

Read-only Opus audits of `feat/rust-rewrite` at `df2d6a4a03` (after #1340).
Findings land as fixes through lanes; strike an item when its PR merges. Line
numbers are as of that commit.

**Judging rule.** Rust need not match TS 1:1. Lifecycle (startup order, dispose,
cleanup, retries, internal state) may differ freely. A divergence matters only
by its production impact: wrong orders, positions, balances or fills; money or
attribution lost; data silently missing or stale; a failure the user cannot see;
or a downstream caller that breaks. Fix to that bar, not to TS's shape.

## Owner decisions (gate the cutover)

1. **Engine delivery to tealstreet-next.** The Copybara glob has no `sdk-rs/**`,
   tealstreet-next already owns a different `sdk-rs/`, and `@tealstreet/safe-cex-wasm`
   builds a gitignored `pkg/`. Mirror under a distinct path plus a wasm build in
   tealstreet-next CI, or publish a prebuilt package.
2. **Keyless hosted signing.** exchange-server asks the browser to sign through
   TS axios interceptors (`base.ts:2074`); Rust refuses remote signers
   (`rust.exchange.ts:397,423`) and the native frame server assumes server-held
   credentials. Engine sign-request actions with browser signers, or server keys.
3. **React Native.** `registry.native.ts` maps 9 names to the wasm engine;
   Hermes has no `WebAssembly`. Revert those to TS for mobile, or a native/JSI host.
4. **Rollback.** `engine` accepts only `'rust'` (`types.ts:137`). Add
   `'typescript' | 'rust'`, a per-venue downstream flag, and a live soak before
   deleting a family.

Already defaulted to Rust with TS removed on this branch: `coinbase`,
`coinbasespot`, the six Orderly brokers, `txflow`. The branch cannot reach master
until 1–3 are settled for them.

## Cutover

- Main-thread writes: web places orders on the main-thread exchange
  (`apps/web/src/order/sendOrder.ts:24`); a main-thread `RustEngineExchange` has
  no engine (`rust.exchange.ts:701`). Route commands to the worker engine.
- No `socketHealth`, `reconnect` or `recoverAfterOffline` on Rust
  (`base.ts:1663,2053`); CLI stale-stream gate fails open
  (`cli-core/src/commands/BaseCommand.ts:172`).
- Rust lacks per venue: `fetchHistoricalPositions`, `fetchClosedPnl`,
  `fetchOrderBook`, Hyperliquid `scheduleCancel`, `updateSymbolMetadata`
  (bitrue, weex), typed GRVT builder error, Hyperliquid `enableSpotMarkets`.
- `instanceof BlofinExchange` downstream (`BlofinExchangeReactiveStateHelper.tsx:20`).
- Deletion inventory misses downstream deep imports: `hyperliquid.constants`,
  `bitget.family`, `asterdex.types`, `bybitinverse`/`bybitspot` exchanges,
  `exchanges/registry`, `exchanges/base`.
- exchange-mcp npm release ships without the engine wasm
  (`.github/workflows/exchange-mcp-release.yml:133`, `tsup.config.ts:36`).
- Combined wasm is 11.7 MB with no budget; no browser cold-start measurement.
  Decide combined vs per-family wasm.
- Replay gate references live TS venues (`harness/venues.mjs`); freeze TS outputs
  as golden fixtures before deleting any TS venue, and drop
  `__tests__/reference/`.

## Trading correctness

High:
- Binance COIN-M treats `EXPIRED` as terminal, dropping a triggered stop's fill
  (`binance/src/account.rs:1186`).
- Binance USD-M amend re-places the full original size when the order filled
  during the cancel ("unknown order" counted as cancel) (`binance/src/trading.rs:1131,536,1463`).
- Rejected private login leaves a silent socket: bitget `account.rs:1128`,
  blofin `account.rs:1125`, mexc `account.rs:308`, woo `account.rs:1482`,
  cryptocom `lib.rs:390`, apexomni `account.rs:1130`. Close and back off; add an auth deadline.
- Binance USD-M emits fills from non-TRADE events and does not dedupe on trade id
  (`binance/src/account.rs:1169`).

Medium:
- Phemex cancel-all fails while any place is in flight (`phemex/src/trading.rs:806`).
- Cancel-all misses just-placed orders: okx, asterdex, breakoutprop, bitget, blofin, bitrue.
- Backpack amend replaces full size after partial fill (`backpack/src/trading.rs:411`).
- No per-order lock between amend and cancel in most venues.
- Stale snapshot reopens canceled orders: Binance USD-M/PM (`trading.rs:1463`),
  Bybit Demo/Hyro (`account.rs:1824`).
- REST snapshot swallows stream fills: bitrue `account.rs:1291`, bingx `account.rs:726`.
- BloFin replays pre-session fills (`blofin/src/account.rs:1231`).
- Stale books stay visible during resync (`engine.rs:1147`); woo ignores `prevTs`,
  kraken spot ignores checksum, asterdex/lighter keep books while waiting.
- Bybit inverse amend writes `remaining` in contracts (`bybit/src/trading.rs:1934`).
- Write timeout discards optimistic state instead of refreshing: orderly, okx.

Low: no reconnect backoff (asterdex, bitrue, bingx, kucoin, cryptocom, phemex,
coinbase public, grvt, paradex, polymarket); KuCoin client-id collisions;
`uuid().unwrap_or_default()` in Binance; Backpack empty positions on any 404;
OKX `partially_effective` final; Lighter late-fill drop; Bybit unused `exec_id`.

Engine root causes: `Fill` has no execution id and the store replaces lists with
no freshness fence, so every venue reinvents dedupe and terminal fencing.

## Wire contract

Repair lane: `feat/rust-wire-fixes`, based on `bd94abb524`. Reverified against
the current code: all requested findings below still hold; ticker orphan
identities already work and must remain intact. Full-auto phases:

1. Preserve wire information: explicit per-section revisions, display symbols,
   orphan identities, order extras, midpoint snapshots/deltas, and owned ticker
   state. Keep Rust, JS declarations, decoder and FrameApplier synchronized.
2. Preserve market identity: symbol-keyed settings and no market-only flush
   while MARKETS is pending; regression tests cover reorder/removal and queued
   books/candles/fills. Route SETTINGS and rare-query leverage writes through
   the sole FrameApplier owner.
3. Recover apply failures: retain independent account sections, stage identity
   changes, request local/worker snapshots, and test actual frame-to-chaser fills.
4. Run scoped native/JS/SDK checks, combined WASM and Binance plus affected-field
   venue replay gates; create the PR into `feat/rust-rewrite`, post adversarial
   reviews, fix findings, merge only on green CI, and cut a follow-up branch.

Each phase requires an independent sub-agent review and a stable commit. Every
fixed finding gets a reproducing regression. No TS venue deletion or replay
threshold relaxation. Strike findings with the fix PR and its evidence.

- ~~FILLS lose `orderId`, so chaser fill tracking never fires on Rust
  (`frameApplier.ts:469`, `ChaserOrderManager.ts:783`).~~
- ~~`Market.pretty_symbol` never encoded (`store.rs:827`).~~
- ~~Rows on markets missing from the table get symbol `''` (`store.rs:704`,
  `frameApplier.ts:337`).~~
- ~~Settings keyed by market index survive a market-table change (`store.rs:144`).~~
- ~~One throw in `FrameApplier.apply` drops the frame with no resync (`frameApplier.ts:41`).~~
- ~~Never encoded: `conditional_order_price`, bracket trigger types,
  `remaining_notional`, `Ticker.mid_price`.~~
- Low: ~~two `fetchedLeverage` writers~~; ~~`FORMAT` never bumped and reserved u16 unused~~;
  fills precede orders by up to 250 ms; ~~`TickerState` views detach on transfer~~.

Repair evidence: actual Rust-writer → JS decoder → FrameApplier regression,
actual FILLS frame → GRVT chaser partial/completion regression, native layout
and market-reorder/queue tests, decoder legacy/revision/bounds/transfer tests,
worker midpoint bridge tests, and local/worker recovery RPC regressions.
FORMAT remains 1 because the envelope is unchanged; section revisions identify
new fixed tails. Both readers check FORMAT; unknown enum values stay absent.
Independent phase reviews found missing STRINGS acceptance, observer exceptions
masking recovery, worker midpoint loss and orphan native-quantity NaNs; all fixed
with regressions. Execution-vs-order flush cadence remains a separate finding.

Phase progress: phases 1–3 and independent reviews complete, committed in
`fb5537d662`. Integrated landing PR #1338 (`1b8592dbd2`) in `a35660f9f3`,
retaining both test additions and adapting renamed helpers/revised Greek tails;
the landed KuCoin Spot leverage path also uses the sole applier owner.
Integrated checks pass: 95 all-feature/89 default engine tests, 25 decoder tests,
187 scoped SDK/worker tests, typecheck/lint, native/WASM Clippy and 586 harness
tests. Combined release WASM built successfully. All 12 public/private gates pass
for Binance, Hyperliquid, OKX, GRVT, KuCoin futures and KuCoin spot.
Public main/steady CPU in ms (TS → Rust): Binance 296.1/166.6 → 158.1/82.4;
Hyperliquid 387.2/44.9 → 81.5/33.8; OKX 1589.2/1188.4 → 1290.8/936.8;
GRVT 171.3/97.4 → 153.7/84.1; KuCoin 408.9/247.4 → 202.6/108.5;
KuCoin spot 525.5/333.2 → 464.3/301.3. All retained-memory comparisons pass.
The first OKX attempt matched output but missed steady CPU by 0.4%; a
same-machine decoder control profile matched output/main CPU, followed by one
full three-run retry with unchanged fixture and thresholds (passing above).
The PR records both attempts and distinguishes profiling from gate evidence.

Phase-4 review additionally found query-only leverage filtered from main, stale
late snapshots, market maxima masquerading as account leverage, and
identity/book baselines surviving rejected frames. Repairs preserve query rails
in coherent snapshot bundles, return only confirmed account rails or null, and
require fresh baselines after errors; each has reproducing SDK regressions.
These are production data-loss/staleness/attribution fixes, not lifecycle parity.
Final phase review passes. Follow-up checks: 54 recovery/worker/contract tests,
12 book tests (two fail with the baseline invalidation removed), 83 JS host/reader
tests, 7 actual-writer cross-language tests, typecheck, production lint and bans.
All requested repairs are ready for the PR adversarial-review and green-CI gates.

## Security

- Workers Durable Object has no client auth and serves the first client's account
  to all (`hosts/wasm/js/workers.mjs:189`); start race leaks running engines (`:206`).
- Native transport errors include signed URLs (`hosts/native/src/transport.rs:390,404`).
- Custom credential headers follow cross-host redirects (`transport.rs:101`, `host.mjs:352`).
- live-smoke: no notional cap, no post-only, testnet reads mainnet env names.
- Per-instance nonces collide across engines on one key: hyperliquid, txflow, kraken.
- Low: `setTickInterval` unclamped; unbounded gunzip; credential `Debug`;
  partial zeroization; Paradex 24 h auth signature; Lighter non-hedged Schnorr
  nonce; denylist tape redaction; `set-cookie` in fixtures; broad gitleaks test exemption.
- Clean: no committed secrets; native frame server auth and isolation; TLS; RNG.

## Architecture

- Numeric decoders duplicated 40× across 19 venues, already divergent → `engine::de`.
- Socket supervision copied per venue, no jitter, two without backoff →
  `engine::socket::Supervisor`.
- Venue registry lives only in the wasm host (`hosts/wasm/src/lib.rs:592`) →
  `safe-cex-venues` crate shared by both hosts.
- Bybit fields and venue-named features in the engine (`model.rs:411`, `engine/Cargo.toml:10`).
- Client ids use OS randomness in the deterministic core (`crypto/src/uuid.rs`).
- ~30 URL encoders in three dialects → `engine::url`.
- `RangeWalk` used by 6 of 26 venues; three book-stitch designs.
- Read-key routing re-implemented ~14 times plus 12 copied test files.
- `docs/plans` 47 files; collapse to this file, `RUST_REWRITE.md`,
  `VENUE_PORT_BRIEF.md`, the cutover docs, and per-venue READMEs. Roster stale.
- Workspace manifest, `rust:*` scripts and `.cargo/config.toml` comment cleanup.

## Gate honesty

No venue has live private evidence; private tapes are generated in CI from
documented or synthetic payloads (14–24 frames for several venues).

- Binance/Bybit oracle copies Rust's `marginSize` into TS when TS is 0
  (`harness/gate.mjs:646`). Circular.
- "Identical output" is final-state only and partial (`harness/replay.mjs:803`):
  no TP/SL, realizedPnl, fees, fill timestamps or execution ids; fill prices
  skipped on binance, bybit, bitget, bingx, cryptocom, mexc, hyperliquid, weex,
  breakoutprop; rows keyed by symbol collapse duplicates (`gate.mjs:87`).
- Whole-section rewrites of the TS side (kraken, backpack, coinbase, orderly,
  kucoin, phemex, cryptocom, bingx, lighter, weex, binance variants); some
  normalizations apply to Rust too and can mask regressions.
- ~20 TS venues were edited on this branch as paired fixes, so the oracle moved
  with the port. Gate against master TS and declare deltas.
- Perf: median of 3 with no margin, harness audit cost inside the window, wasm
  compile outside it, process CPU ungated (Rust loses on grvt, hyperliquidspot,
  bybit, woo, breakoutprop), failed perf jobs re-run to green, retained memory
  mostly TS module graph and redefined ~10 times.
- Size budget covers per-venue builds; the shipped combined build (≈10–11.7 MB)
  has none. Paradex has 678 bytes of headroom.
- Public fixtures are recorded by driving Rust, so TS-only subscriptions cannot
  diverge; fixtures refreshed alongside behaviour fixes.
- `bitgetusdc` has no CI replay gate; HyroTrader and BreakoutProp public data are synthetic.

Before cutover: live-smoke private captures on every venue (mainnet minimum size
for coinbase, backpack, mexc with approval), a 3–7 day read-only production
shadow diffing full store snapshots, public fixtures re-recorded with TS, and a
per-venue canary holding the circular-evidence venues last.

## Port reviews (Opus, per venue vs TS)

### OKX
- High: blank `bidPx`/`askPx`/volume or any failed instruments/tickers read at
  startup halts the venue with no retry (`okx/src/lib.rs:841,995`); TS treats
  blanks as 0 and continues. Parse blanks as 0, retry with backoff.
- Medium: one bad instrument row aborts the catalogue (`lib.rs:738`); leverage
  for flat symbols missing (no `leverage-info`); account config (posMode, acctLv)
  read once, so a mode change in the OKX app makes every order reject.
- Low: `mmp`/`elp` order types get an empty id; conditional `price: 0`
  rejected; income limit truncates.
- Confirm (TS bugs Rust fixed): OCO leg edit deleting its pair, triggered algos
  staying open, X-Perp history SWAP hardcode, market-order amend placing a new
  order, duplicate algo cancels. Shared by both: reduce-only conditional sent
  without `reduceOnly` and un-inverted `posSide` in hedge mode.

### Hyperliquid
Signing hash, order payload and price/size rounding match TS; no new highs.
- Medium: HIP-3 leverage change sends cross where TS sends isolated
  (`hyperliquid/src/trading.rs:758`); read `onlyIsolated`/`marginMode`. HIP-3
  margin-type change with no position fails (default leverage 20 > market max,
  `trading.rs:759`). Spot open orders never reconciled and can go stale
  (`account.rs:426,1604`). Edit aborts when the source order already filled
  (TS places a second order) — confirm fail-closed is intended.
- Low/confirm: spot testnet signing and spot size rounding (TS bugs Rust fixed);
  spot vault signs as vault (TS as master) — confirm; more terminal statuses;
  `stopLoss: 0` rejects whole order; signed fees; unified balance spot slices
  not populated; fills lack execution id, realized PnL and fee (engine `Fill`).

### Binance family
- The known USD-M amend double exposure also hits PM, where TS throws on -2011
  and Rust re-places (`binance/src/trading.rs:530`): a regression.
- Medium: PM upserts `fs:"CM"` rows as phantom positions (`account.rs:1381`);
  REST candles with `from`+`limit` return the oldest N instead of newest, no
  1500 cap (`lib.rs:1439`); COIN-M strict batch discards placed ids on a partial
  reject (`trading.rs:397,700`); PM `close: true` with an amount closes the whole
  position (`trading.rs:1087`); COIN-M unpriced coin excluded from totals
  (TS counts 1:1 — confirm); CancelOrders with unknown ids reports success
  without sending (`trading.rs:224`); history maps `EXPIRED_IN_MATCH` to filled
  (`history.rs:687`); spot/COIN-M history returns plain `[]` instead of
  unsupported; USD-M journal (`fetchHistoricalPositions`) not wired in the host.
- Low: fixed `recvWindow`; closePosition conflict cancel in batches; Binance UI
  STOP orders amend to stop-market; PM -2011 plain cancel marks canceled.
- Confirm (TS bugs Rust fixed): fill price `L` not `ap`; spot fill price on
  market orders; spot cancel uses `C`; amend keeps TIF; PM conditional client id
  `newClientStrategyId`; PM TRIGGERED closes; PM cancel-all both endpoints;
  COIN-M rejects caller client ids; spot retired streams; depth snapshot retry.

### Bybit family
- High: Brazil inverse/option sockets routed to the linear host (engine
  transport keys routes by channel, `packages/safe-cex/src/transport/engine.ts:552`,
  `bybit/src/lib.rs:407`); per-coin balances never set (`account.rs:662`);
  fills history includes funding/settlement rows (`history.rs:564`); income
  dedupe drops hedge-mode funding legs (`history.rs:131`).
- Medium: option failures stall linear startup (`lib.rs:857`); option catalog
  `baseCoin=All` unverified; live candles lack trade aggregation and seconds
  intervals; option WS ticker field names likely wrong; no UNIFIED retry on the
  wallet call; demo reduce-only fill check missing; Hyro tears down on one bad
  poll (`account.rs:1838`); order/income history caps; Partial TP/SL amend
  semantics (owner decision); spot create params (`marketUnit`); one unmappable
  order invalidates every later snapshot (`account.rs:1569`).
- Low: OHLCV from+limit; no market refresh; Hyro market filter; subscription
  breadth; split-lot error handling; classic upnl; inverse reference price;
  demo key hint, recvWindow, rate limiter; Hyro early pings.
- Confirm (TS bugs Rust fixed): spot amount precision, option books socket, spot
  cancel-all groups, spot trailing/full TP/SL endpoint, inverse position-mode
  category, stop with only price, category-filtered private topics,
  PartiallyFilledCanceled, inverse units, failed position read zeroing, create
  network errors, live candle volume units.

### BloFin
- High: a failed or malformed ticker read at startup means the private side
  never starts, no retry (`blofin/src/lib.rs:617,651`); `brokerId` never sent,
  losing broker attribution (`trading.rs:126`, TS `blofin.exchange.ts:2159`);
  per-symbol margin mode stays stale after a position closes, so orders go out
  in the wrong margin mode (`trading.rs:271`, `account.rs:614`).
- Medium: `null` in order/position/ticker fields rejects whole snapshots/frames
  (`wire.rs`; verify against one live frame); WS login timestamp serialized as
  float (`account.rs:284`); one odd instrument row aborts startup
  (`lib.rs:505`); cancel of an id the store lacks resolves OK with no request
  (`trading.rs:671`).
- Low: strict leverage fetch; trigger TP/SL not tick-rounded; missing
  iso/cross warnings; 102143 not translated; history shape.
- Confirm (TS bugs Rust fixed): IOC/FOK rejected; TP/SL trigger type honored;
  zero-size partial TP/SL rejected; amend checks order-detail (endpoint
  unexercised live); per-row cancel errors; empty snapshot commits; income
  filters; boolean reduceOnly.

### Cross-venue patterns from the port reviews
- Startup is fail-closed with no retry on OKX and BloFin where TS continues on
  partial data; check every venue and set one engine policy.
- Broker attribution: BloFin drops `brokerId`; check every venue's broker/referral
  tag against TS.
- Cancel of ids not in the store silently succeeds (Binance, BloFin).

### Bitget family
- High: hedge-mode isolated leverage always goes to the long leg — the adapter
  drops `side` (`rust.exchange.ts:822`), `SetLeverage` has no side
  (`engine/src/command.rs:85`), Bitget hard-codes `long` (`trading.rs:548`).
  Affects every venue with `hasIndependentHedgedLeverageInIsolatedMargin`.
- Medium: Classic fills/order history fail outright on liquidation/ADL/reduce
  fills (`history.rs:984,867`); one bad market row or a transient error halts
  startup with no retry (`lib.rs:552,684`); one unmappable row discards the
  positions/orders snapshot (`account.rs:505,806`).
- Low: zero-lot order rejects the whole batch; commands before the account probe
  rejected; sized stop silently becomes full-position; WS-triggered Classic
  refresh dropped while another is in flight; `"0"` leverage limits make `clamp`
  panic (`trading.rs:547`); UTA strategy ids look virtual; UTA one-way fills
  report side `both`.
- Confirm (TS bugs Rust fixed): Classic limit move dropping attached TP/SL; UTA
  attached TP/SL edit cancelling the parent; hedge plan side flip; close orders
  side inversion (verify live); USDC commission asset.

### Engine and TS bridge
Verdict: reads and streaming match; writes and lifecycle do not yet. Blocks
further cutover and warrants re-checking coinbase, Orderly, txflow.
- High: cancel sends ids only and venues drop ids not yet in their store, so a
  chaser stop during placement leaves the order live (`rust.exchange.ts:789`,
  `bybit/src/trading.rs:422`); `setLeverage` drops side (see Bitget); a wasm
  panic freezes the account silently — no containment in `host.mjs:131,175`;
  `fetchMarkets`/`fetchTickers` on an unstarted instance return `[]` and
  `PublicExchangeFactory` caches it (`rust.exchange.ts:856`); book/candle
  subscriptions made before the engine starts are lost (`rust.exchange.ts:1000,1022`).
- Medium: ~~`fetchLeverageForSymbol` returns market max for mexc, bingx, weex
  (`rust.exchange.ts:945`) and frame mode strips `fetchedLeverage`~~; command
  rejections don't emit the error toast (one-error rule); `privateOnly`/
  `externalPublicData` ignored; `getAccount` missing for ~18 venues; dispose
  mid-command reports a placed order as failed (`host.mjs:83`); base
  `cancelAllOrders`/`nuke` read the 250 ms-lagged TS store; fills lack
  realizedPnl/fee; `fetchOrders`/`fetchPositions`/`removeOrders` don't reach the
  engine (needs a resync command); `validateAccount` leaves a started engine.
- Low: Asterdex unsupported interval mapped to 1h breaks candle matching;
  `num::adjust` with step ≤ 0; no command deadline, `execute` drops the batch tail
  on an unknown action; candles drop `lastSide`, orders lack generic `triggerType`.

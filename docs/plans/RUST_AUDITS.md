# Rust rewrite audits

Read-only Opus audits of `feat/rust-rewrite` at `df2d6a4a03` (after #1340).
Findings land as fixes through lanes; strike an item when its PR merges. Line
numbers are as of that commit.

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

- FILLS lose `orderId`, so chaser fill tracking never fires on Rust
  (`frameApplier.ts:469`, `ChaserOrderManager.ts:783`).
- `Market.pretty_symbol` never encoded (`store.rs:827`).
- Rows on markets missing from the table get symbol `''` (`store.rs:704`,
  `frameApplier.ts:337`).
- Settings keyed by market index survive a market-table change (`store.rs:144`).
- One throw in `FrameApplier.apply` drops the frame with no resync (`frameApplier.ts:41`).
- Never encoded: `conditional_order_price`, bracket trigger types,
  `remaining_notional`, `Ticker.mid_price`.
- Low: two `fetchedLeverage` writers; `FORMAT` never bumped and reserved u16 unused;
  fills precede orders by up to 250 ms; `TickerState` views detach on transfer.

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

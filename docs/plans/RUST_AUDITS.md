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
- Medium: `fetchLeverageForSymbol` returns market max for mexc, bingx, weex
  (`rust.exchange.ts:945`) and frame mode strips `fetchedLeverage`; command
  rejections don't emit the error toast (one-error rule); `privateOnly`/
  `externalPublicData` ignored; `getAccount` missing for ~18 venues; dispose
  mid-command reports a placed order as failed (`host.mjs:83`); base
  `cancelAllOrders`/`nuke` read the 250 ms-lagged TS store; fills lack
  realizedPnl/fee; `fetchOrders`/`fetchPositions`/`removeOrders` don't reach the
  engine (needs a resync command); `validateAccount` leaves a started engine.
- Low: Asterdex unsupported interval mapped to 1h breaks candle matching;
  `num::adjust` with step ≤ 0; no command deadline, `execute` drops the batch tail
  on an unknown action; candles drop `lastSide`, orders lack generic `triggerType`.

### Kraken
Futures matches TS on nearly every input; no high regression.
- Medium: isolated positions flip to cross when a WS row omits
  `maxFixedLeverage` (`kraken/src/account.rs:1056,1132`; confirm live);
  `more_candles: true` fails the whole candle fetch, empty chart (`lib.rs:727`);
  spot `EGeneral:Permission denied` on an order deactivates the account
  (`spot/account.rs:513`).
- Owner decision: market entry with attached TP/SL — TS silently drops the
  legs, Rust rejects the order.
- Low: income over 500 rows errors (TS partial); one bad row rejects
  catalogue/tickers/open orders and blocks private start (startup lane);
  missing funding field set to 0; raw error text; spot 3m candles.
- Confirm (TS bugs Rust fixed): index trigger signal; full_fill not canceled;
  live % change/volume/OI; stop amend racing a fill; spot fills over-reported
  and mislabelled XBT/BTC.

### Orderly and TxFlow (already default to Rust)
TxFlow loses nothing (master TS was a placeholder).
- High (Orderly, all six brokers): standalone SL/TP rejected because the app
  always sends `triggerPrice: price` (`orderly/src/trading.rs:362`); master TS
  ignored `triggerPrice`. Chart-drag stops, stop form and hotkeys all fail.
- Medium: cancel-all only loops symbols in the store instead of the unscoped
  DELETE (`rust.exchange.ts` has no Orderly override; engine supports
  `symbol: ''`); SL/TP blocked when a trailing stop or quantity TP_SL exists on
  that side (`trading.rs:254`).
- Low: one bad market row or unknown order row fails closed (startup lane);
  trailing-stop display price; cancels skipped for unknown/finished ids;
  candle `from`/`to` filtering a latest-N fetch.

### KuCoin
- High: cancel-symbol also deletes the symbol's TP/SL stops
  (`kucoin/src/trading.rs:515`; TS only `/api/v3/orders`); one failed or
  malformed markets/tickers read stops the venue, and futures private REST
  waits for the private socket `welcome` (`lib.rs:767,954,1026`).
- Medium: one unknown id rejects the whole cancel batch (`trading.rs:505,1034`);
  one malformed REST row freezes reconciliation (`account.rs:652`); caller client
  id on attached TP/SL rejected (`trading.rs:120`).
- Low: leverage sent as `10.0`; fill without `matchPrice` dropped; spot polls
  5 s vs 500 ms; per-symbol cancel-all; partial balance rows ignored.
- Confirm (TS bugs Rust fixed): stop side read-back, amend overfill, amend
  keeping trigger type and post-only, standalone stop client id, open-order
  paging, spot partial snapshot, swallowed cancel errors.

### Cross-venue: app stop payloads
The app sends stops with `triggerPrice` equal to `price`. Orderly rejected every
such stop. Many venues test `trigger_price.is_some()` in placement (apexomni,
backpack, binance, bingx, bitget, bitrue, blofin, coinbase, breakoutprop, bybit,
…); each must accept the app's exact payloads.

### Phemex
- High: `SetPositionMode` has no symbol (`engine/src/command.rs:95`), so Phemex
  loops PUTs over every market and can half-switch (`phemex/src/trading.rs:858`);
  one unknown order status/type drops the whole private frame incl. positions
  and balances (`account_wire.rs:298`, `account.rs:601`).
- Medium: one bad public row stops startup; one bad position row freezes a
  currency slice (`account.rs:286`); no fallback read when a symbol's mode or
  leverage context is uncached (`trading.rs:229,713`).
- Low: cancel batch refused on one uncached id; index triggers refused; daily
  candle `to`; no 1 s REST resync after terminal events.
- Confirm (TS bugs Rust fixed): filled from cumQty with exec dedupe; WS-closed
  positions removed; amend re-places remainder; hedged margin per side; split
  stops at first failure.

### MEXC
- High: live tickers only for the first 100 markets and `SetTickerSymbols`
  ignored (`mexc/src/lib.rs:416`), so positions outside them keep startup prices
  forever; one open order on an unloaded market blocks the order store for good
  (`account.rs:618,850`).
- Medium: full-position TP/SL `vol` not snapped to step (`trading.rs:342`); every
  single order uses the batch path paced 1.5 s (`trading.rs:451`); startup halts
  on tickers/markets/position-mode/plan-list failures.
- Low: every plan order marked reduce-only; signed fill fee; `fetchOrderBook`
  refused by the adapter.

### Cross-venue: rows and subscriptions
- One unmappable row discarding a whole snapshot/frame recurs (Bitget, Phemex,
  MEXC, KuCoin, Kraken, Orderly, Bybit): skip and log the row, keep the store's
  copy of that id.
- Check every venue honours `SetTickerSymbols` (MEXC ignores it).

### BingX
Normal inputs send the same requests as TS; the risk is resilience.
- High: one bad market/ticker row or failed funding read keeps the private side
  from ever starting (`bingx/src/lib.rs:529,639`); one bad position/order row
  freezes that list (`account.rs:491,557`). (startup/row-tolerance lane)
- Medium: `TRIGGER_MARKET` stops (attached brackets) can't be moved
  (`trading.rs:842`); no one-shot resend on signature error 100001; multi-order
  cancel sends nothing if any id is unknown (`trading.rs:780`).
- Low: hedge leverage sets both sides; `cancelRestrictions=ONLY_NEW` on amend;
  per-symbol leverage/margin not read; trigger move requires trigger_type;
  bracket `workingType`; history fails on one bad row; WS message with one bad
  field dropped.

### WEEX
- High: one `null` numeric field in a public row stops the venue, private start
  gated on tickers (`weex/src/wire.rs:73`, `lib.rs:671,727`); one unparsable
  order/position row freezes the list, no `planType` fallback (`account.rs:335`).
- Medium: failed config read backs off all polling to ~60 s (`account.rs:518`);
  balance stops updating during active trading (revision discard,
  `account.rs:408`); accepted conditional reported failed if the venue echoes
  `clientAlgoId` (`trading.rs:1303`; verify live); imported limit with TP/SL
  can't be amended (`trading.rs:343`); missing passphrase looks like a valid
  empty account (`lib.rs:148`).
- Low: isolated leverage sets both sides; index triggers rejected; fill without
  `id` dropped; income row without asset fails the page; fetchLeverage returns max.

### Coinbase (already defaults to Rust)
- High: one unmappable open order (e.g. a bracket or TWAP placed on Coinbase's
  site) rejects every snapshot and blocks all trading incl. cancel
  (`coinbase/src/account.rs:336,1577,1842`, `trading.rs:1028`); a failed
  placement leaves a reservation that breaks cancel-symbol for the session
  (`trading.rs:1109,1283,1164`); CLI market orders with GTC rejected
  (`trading.rs:264`, `cli-core/.../BaseBuySellCommand.ts:368`); browser-side
  `signWsAuth`/HTTP signing lost (decision 2).
- Medium: reduce-only capacity counts protective stops against TPs and spot
  reduce-only always rejected (`trading.rs:654,676`); portfolio probe stalls on
  404/other non-2xx (`account.rs:1322,1687`); one bad CFM position row rejects
  the snapshot (`account.rs:1246`).
- Low: CFM `upnl` and spot `total` sources differ (equity double-count risk);
  OHLCV honours `from`; placement errors lose Coinbase's message.

### Lighter
Signing, integer scaling, tx encoding and nonces match TS.
- High: position `initial_margin_fraction` ("20.00" = 5x) read in basis points
  (`lighter/src/account.rs:424,877`), so leverage reads as the market max and a
  margin-mode change signs max leverage (`trading.rs:567`). Apply TS's three-way
  rule to the position field; add a `"20.00"` test.
- Medium: windowed fill history pages the whole account history then fails
  (`history.rs:404,744`); one bad market/account row stalls startup or sync;
  account tier not exposed and `changeAccountTier` missing.
- Low: thinner error mapping (rate-limit guidance, 21613 refresh); reduce-only
  re-check races a fixed 2 s timer. Shared with TS: amend re-places the full
  original size after a partial fill.

### WOO
No confirmed highs; valid data matches TS.
- Medium: algo child rows don't inherit the parent's side/status/reduceOnly, so
  fills can be lost (`woo/src/account.rs:700`; verify one live frame); one bad
  position row freezes positions (`account.rs:749,949`); one unmappable
  fill/order-history/instrument/ticker row fails the read or startup.
- Low: SL trigger falls back to `triggerType`; algo reduceOnly default; leverage
  readback can report a false error; isolated warning; stop without reference
  price rejects the snapshot.

### AsterDEX
No confirmed highs.
- Medium: failed position-mode read silently leaves one-way, never retried, and
  orders can go before it lands (`asterdex/src/account.rs:566`); order events
  with unmapped types (e.g. `LIQUIDATION`) dropped with their fills
  (`account.rs:817`); one malformed row halts startup (`lib.rs:849,781`); one bad
  row discards open-orders/positions snapshots (`account.rs:402,415`); hedge
  amend of a closing order may open the opposite side (shared with TS; reuse the
  row's `positionSide`).
- Low: no credential check at start; ACCOUNT_UPDATE strict on `ep`/`cr`; fills
  history fails on one row and refuses >7 day windows; listen-key keepalive
  omits `listenKey` and tears down on failure; partial balance rows skipped.

### Crypto.com
Mostly matches TS.
- Medium: per-coin balances (`position_balances`) dropped
  (`cryptocom/src/account.rs:232`); caller client id refused on stop/TP orders
  (`trading.rs:76`).
- Low: one invalid order fails the whole batch; null string fields fail reads;
  one bad instrument row stops startup.

### Bitrue
- High: orders refused on symbols with no account leverage data, and a fresh
  account can never trade (`bitrue/src/trading.rs:363`, `account.rs:948`);
  default margin to Cross and take leverage from a successful change, never
  the market max. A cancel batch stops at its first failure and leaves the rest
  live (`trading.rs:866,884`).
- Medium: orders marked loaded only after every market's open-orders read
  succeeds, one bad symbol blocks forever (`account.rs:382,487`); per-coin
  balances dropped; max leverage stuck at 100 with silent clamping (no
  leverage bracket read, `lib.rs:758`).
- Low: one bad contract row blocks startup; stream stops misclassified;
  snapshot starvation under load (revision discard, `account.rs:581,1310`);
  malformed snapshot never sets loaded.

### Cross-venue: per-coin balances and snapshot starvation
- Per-coin balances missing on Bybit, Crypto.com, Bitrue; check every venue.
- Revision-discarded snapshots starve under active trading on WEEX and Bitrue;
  check every venue that fences REST snapshots by stream revision.

### ApeX Omni
Signing and payloads match; no confirmed highs.
- Medium: cancelled ids hidden for good (terminal map, `apexomni/src/account.rs:150`;
  cancel-all marks untriggered stops terminal, `trading.rs:928`); one bad
  open-order row leaves the account side dead incl. signing state
  (`account.rs:975`); null fields drop whole private frames incl. fills
  (`private_wire.rs:205`); one odd `/v3/symbols` row blocks private start; a bad
  omni key seed kills read-only monitoring (`account.rs:504`).
- Low: caller client id rejected on stops/brackets/splits; float remaining size;
  zero-price candle rejects batch; leverage always sent. Unchecked: private
  socket via Tealstreet proxy.

### Paradex
StarkNet typed data, JWT, payloads and SBE decoding match.
- High: one blank ticker field halts the venue incl. private side
  (`paradex/src/public.rs:714`, `lib.rs:127`); a failed TP/SL child logs at
  Warning so no toast — user believes they are protected (`trading.rs:855`);
  REST positions may never commit if `leverage`/`liquidation_price` are blank
  (`account.rs:1824`; verify live).
- Medium: multi-order placement stops at first failure and loses placed ids
  (`trading.rs:458,699,867`); venue rejection reasons dropped (`lib.rs:231`);
  leverage serialized `20.0` (`trading.rs:542`; verify live); one malformed SBE
  frame recycles the public socket (`public.rs:388`).
- Low: one unmappable order row discards refresh; leverage-0 config row; TP/SL
  of 0 rejects order; history fails on one row; stale uPnL after WS close;
  README says remote signing rejected.

### Cross-venue: silent partial failures
- Child-leg failures (TP/SL after an entry) must surface as errors on every
  venue (Paradex logs at Warning).
- Batches that fail midway must return placed ids with the error (Paradex,
  Binance COIN-M, Bitrue cancels).
- Integers serialized as floats (`20.0`): Paradex leverage, KuCoin leverage;
  check every venue's integer fields.

### GRVT
Signing and integer scaling exact; no highs.
- Medium: builder check fails once → every order rejected for the session, and
  a matching row without `mfr` treated as unknown (`grvt/src/account.rs:1531,1011`,
  `trading.rs:777`); one unknown-instrument row hides all positions or orders
  (`account.rs:516,422,970`); GRVT's rejection message dropped and misleading
  "reconcile" advice added on definitive rejects (`wire.rs:180`, `trading.rs:1348`).
- Low: order history fails on one bad row; signer mismatch blocks trading;
  1008 loses the proxy-whitelist message; amend may cancel without replacing if
  cancels are asynchronous (verify live).

### Polymarket (development-only)
CLOB V2 signing and amounts match the official client.
- Medium: one `null` in a Gamma market row breaks the venue with no retry
  (`polymarket/src/wire.rs:33`, `lib.rs:688,832`).
- Low: cancel-all reports failure after a successful cancel; one bad position
  row freezes positions; auth failures invisible.

### Registration and options (all RUST_VENUES aliases)
Alias→product mapping, credentials, testnet, routing and broker/builder
attribution are correct for all 48 names (BloFin `brokerId` aside).
- Medium: Coinbase accounts with a saved read-only key fail to start — no
  `secondaryReadHeaders` entry or `read_credentials` (`transport/engine.ts:76,116`,
  `hosts/wasm/src/lib.rs:656`); Coinbase is Rust-only.
- Low/medium: polling controls ignored — `extra.tickInterval` (incl. `Infinity`
  to stop polling), the server slowdown multiplier and credential-rejection
  backoff reach only BreakoutProp (and raw tickInterval to Orderly, where
  `Infinity` becomes `null`) (`rust.exchange.ts:308,512`); every other venue polls
  a fixed constant, so the rate-limit incident lever does nothing on Rust.

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

Pending.
